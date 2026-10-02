#pragma once
// Arithmetic from historical spike 1aa229c; table supplied at runtime.
#include "defs.h"

void decrypt_main(const uint8 key_basis[16], uint8 dst[16]);
void bind_key(const uint8 decrypted_key[16], const uint8 file_id[20], uint8 dst[16]);
uint32 process(char a1, uint32 a2, uint32 a3, uint32 a4, uint32 a5);

template <typename A>
unsigned char _bittest(A *a, int b)
{
    unsigned char *bits = (unsigned char *)a;
    unsigned char value = bits[b >> 3];
    unsigned char mask = (unsigned char)(1 << (b & 7));
    return (value & mask) != 0;
}

static unsigned int U8TOU32(const unsigned char *p) {
    return (((unsigned int)(p[0] & 0xff)) | ((unsigned int)(p[1] & 0xff) << 8) |
            ((unsigned int)(p[2] & 0xff) << 16) | ((unsigned int)(p[3] & 0xff) << 24));
}


extern uint32 playIntentKey[768];
