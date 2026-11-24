// harlo's use of TweetNaCl (tweetnacl.c, public domain, from
// https://tweetnacl.cr.yp.to/20140427/): NaCl's box, as the phone protocol
// uses it (crates/harlo-core/src/phone.rs), without the zero padding.
#ifndef HARLO_NACL_H
#define HARLO_NACL_H

#include <stddef.h>

#define HARLO_KEY_BYTES 32
#define HARLO_NONCE_BYTES 24
#define HARLO_MAC_BYTES 16

/// The public key (32 bytes) of a secret key (32 bytes).
void harlo_public_key(unsigned char *pk, const unsigned char *sk);
/// The shared key between their public key and our secret key. 0: ok.
int harlo_box_shared(unsigned char *k, const unsigned char *pk, const unsigned char *sk);
/// Seals `m` (`len` bytes) into `c` (`len + 16` bytes: MAC, then text). 0: ok.
int harlo_box_seal(unsigned char *c, const unsigned char *m, size_t len, const unsigned char *nonce, const unsigned char *k);
/// Opens `c` (`len` bytes, at least 16) into `m` (`len - 16` bytes). 0: ok,
/// -1: it didn't verify (tampered, or the wrong keys).
int harlo_box_open(unsigned char *m, const unsigned char *c, size_t len, const unsigned char *nonce, const unsigned char *k);

#endif
