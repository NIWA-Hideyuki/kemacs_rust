/*
 * test_kgetc_esc.c -- Verify the kgetc() ESC/SI/SO input state machine.
 *
 * Regression guard for: a bare ESC (the Meta prefix, e.g. M-g) set
 * KS_CODE = KS_JIS and never reset it, so a following Ctrl-N/Ctrl-O
 * (SI/SO) was swallowed as a JIS locking-shift escape.  KS_JIS must be
 * committed only when a real JIS escape (ESC $ ... or ESC ( ...) is seen.
 */

#include "test.h"
#include <stdlib.h>
#include <string.h>

#include "../kanji/kanji.h"
#include "../kanji/wcwidth.c"
#include "../kanji/stoj.c"
#include "../kanji/jtos.c"

/* Expose static functions from kgetc.c */
#define static
#include "../kanji/kgetc.c"
#undef static
#undef JKANJI
#undef KANA
#undef NORMAL

/* Expose static functions from kputc.c (provides kputc_iconv_close etc.) */
#define static
#include "../kanji/kputc.c"
#undef static
#undef JKANJI
#undef KANA
#undef NORMAL

/* ---- in-memory input stream ---- */
typedef struct { const unsigned char *data; int len; int pos; } IN;

static int
myget(caddr_t id, char *buf, int len)
{
    IN *in = (IN *)id;
    int avail = in->len - in->pos;
    int n;
    if (avail <= 0) return -1;          /* EOF -> KST_EOF, stops re-read */
    n = (len < avail) ? len : avail;
    memcpy(buf, in->data + in->pos, (size_t)n);
    in->pos += n;
    return n;
}

/* Feed bytes through kgetc and collect everything it returns (excluding EOF). */
static int
run_kgetc(const unsigned char *data, int len, int *out, int outcap)
{
    KSTREAM ks;
    IN in;
    int n = 0, c;

    memset(&ks, 0, sizeof ks);
    ks.ks_id = (caddr_t)&in;
    ks.ks_getf = myget;
    in.data = data; in.len = len; in.pos = 0;
    /* stdin starts in UTF-8 mode (tcap.C_:227), THRU keeps high bytes. */
    KS_CODE(ks.ks_flag) = KS_UTF8;
    KS_THRU(ks.ks_flag) = 1;
    KS_INTERP(ks.ks_flag) = KS_KANJI;

    while ((c = kgetc(&ks)) != EOF && n < outcap)
        out[n++] = c;
    return n;
}

static const unsigned char S1[] = { 0x1b, 'g', 0x0e };   /* ESC g Ctrl-N */
static const unsigned char S2[] = { 0x1b, 'g', 0x0f };   /* ESC g Ctrl-O */
static const unsigned char S3[] = { 0x1b, '$', '@', 0x21, 0x31 }; /* JIS X0208 */
static const unsigned char S4[] = { 0x1b, '(', 'I', 0x24 };       /* JIS X0201 kana */
static const unsigned char S5[] = { 0x1b, '$', '@', 0x21, 0x31, 0x0e, 'A' };
static const unsigned char S6[] = { 0x1b, 'z', 0x0e };   /* unknown ESC seq */

int
main(void)
{
    int out[64]; int n;

    /* 1) Bare ESC + 'g' + SI(0x0e): all three must come through.
       (Ctrl-N == SHFTOU == 0x0e in kemacs naming.) */
    n = run_kgetc(S1, sizeof S1, out, 64);
    TEST_ASSERT_INT(n, 3, "bare ESC+g+Ctrl-N not swallowed");
    TEST_ASSERT_INT(out[0], 0x1b, "ESC passes");
    TEST_ASSERT_INT(out[1], 'g', "'g' passes");
    TEST_ASSERT_INT(out[2], 0x0e, "Ctrl-N (SI) delivered, not swallowed");

    /* 2) Bare ESC + 'g' + SO(0x0f): likewise. */
    n = run_kgetc(S2, sizeof S2, out, 64);
    TEST_ASSERT_INT(n, 3, "bare ESC+g+Ctrl-O not swallowed");
    TEST_ASSERT_INT(out[2], 0x0f, "Ctrl-O (SO) delivered, not swallowed");

    /* 3) JIS X 0208: ESC $ @ <b1> <b2> -> one combined JIS code (b1<<8 | b2). */
    n = run_kgetc(S3, sizeof S3, out, 64);
    TEST_ASSERT_INT(n, 1, "ESC $ @ decodes one 2-byte kanji");
    TEST_ASSERT_INT(out[0], (0x21 << 8) | 0x31, "ESC $ @ combined bytes into JIS code");

    /* 4) JIS X 0201 kana: ESC ( I <kana> -> enkana (0x0100-0x017f). */
    n = run_kgetc(S4, sizeof S4, out, 64);
    TEST_ASSERT_INT(n, 1, "ESC ( I decodes one kana");
    TEST_ASSERT(out[0] >= 0x0100 && out[0] <= 0x017f, "ESC ( I produced half-width kana");

    /* 5) After a real JIS escape, SI still toggles G1 (committed KS_JIS):
          ESC $ @ b1 b2 then SI then 'A' -- SI consumed (no extra output byte). */
    n = run_kgetc(S5, sizeof S5, out, 64);
    TEST_ASSERT_INT(n, 2, "real JIS: SI swallowed, then G1 char follows");

    /* 6) Unrecognized ESC seq (ESC z) passes ESC,'z' through, KS_CODE stays UTF8,
          so a following Ctrl-N is still delivered. */
    n = run_kgetc(S6, sizeof S6, out, 64);
    TEST_ASSERT_INT(n, 3, "ESC z + Ctrl-N: all pass through");
    TEST_ASSERT_INT(out[2], 0x0e, "Ctrl-N delivered after bare ESC-z");

    printf("\nResults: %d passed, %d failed (%d total)\n",
           test_passed, test_failed, test_count);
    return test_failed ? 1 : 0;
}
