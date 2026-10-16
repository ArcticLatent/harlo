#include "harlo_nacl.h"
#include "tweetnacl.h"

#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// TweetNaCl's own key generation wants this; harlo makes its keys and
// nonces in Swift, so it's only here to link.
void randombytes(unsigned char *x, unsigned long long n) {
#if defined(__APPLE__)
    // (iOS has no getentropy header.)
    arc4random_buf(x, (size_t)n);
#else
    while (n > 0) {
        size_t chunk = n > 256 ? 256 : (size_t)n;
        if (getentropy(x, chunk) != 0)
            abort();
        x += chunk;
        n -= chunk;
    }
#endif
}

void harlo_public_key(unsigned char *pk, const unsigned char *sk) {
    crypto_scalarmult_curve25519_tweet_base(pk, sk);
}

int harlo_box_shared(unsigned char *k, const unsigned char *pk, const unsigned char *sk) {
    return crypto_box_curve25519xsalsa20poly1305_tweet_beforenm(k, pk, sk);
}

// TweetNaCl's box works on padded buffers: 32 zero bytes before the text,
// 16 before the sealed text.
int harlo_box_seal(unsigned char *c, const unsigned char *m, size_t len, const unsigned char *nonce, const unsigned char *k) {
    size_t n = len + 32;
    unsigned char *pm = calloc(n, 1), *pc = calloc(n, 1);
    int r = -1;
    if (pm && pc) {
        memcpy(pm + 32, m, len);
        r = crypto_box_curve25519xsalsa20poly1305_tweet_afternm(pc, pm, n, nonce, k);
        if (r == 0)
            memcpy(c, pc + 16, len + 16);
    }
    free(pm);
    free(pc);
    return r;
}

int harlo_box_open(unsigned char *m, const unsigned char *c, size_t len, const unsigned char *nonce, const unsigned char *k) {
    if (len < 16)
        return -1;
    size_t n = len + 16;
    unsigned char *pc = calloc(n, 1), *pm = calloc(n, 1);
    int r = -1;
    if (pc && pm) {
        memcpy(pc + 16, c, len);
        r = crypto_box_curve25519xsalsa20poly1305_tweet_open_afternm(pm, pc, n, nonce, k);
        if (r == 0)
            memcpy(m, pm + 32, len - 16);
    }
    free(pc);
    free(pm);
    return r;
}
