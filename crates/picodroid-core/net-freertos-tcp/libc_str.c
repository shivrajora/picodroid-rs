/*
 * Minimal libc string shims for bare-metal builds.
 *
 * thumbv8m.main-none-eabihf links without newlib, so the FreeRTOS+TCP
 * DNS / sockets paths and cyw43-driver reference string functions that
 * aren't provided by compiler_builtins. Keep the implementations small
 * and obvious — correctness matters more than speed at these call sites
 * (DHCP hostname parsing, event-name table lookups, DNS reply name checks).
 */

#include <stddef.h>

int strcmp(const char *a, const char *b) {
    while (*a && *a == *b) { a++; b++; }
    return (unsigned char)*a - (unsigned char)*b;
}

int strncmp(const char *a, const char *b, size_t n) {
    while (n && *a && *a == *b) { a++; b++; n--; }
    if (n == 0) return 0;
    return (unsigned char)*a - (unsigned char)*b;
}

/* ASCII-only, which is all a DNS name holds (FreeRTOS+TCP V4.4.2 compares
 * the name in a reply against the one asked for). */
int strcasecmp(const char *a, const char *b) {
    for (;; a++, b++) {
        int ca = (unsigned char)*a, cb = (unsigned char)*b;
        if (ca >= 'A' && ca <= 'Z') ca += 'a' - 'A';
        if (cb >= 'A' && cb <= 'Z') cb += 'a' - 'A';
        if (ca != cb || ca == 0) return ca - cb;
    }
}

char *strcpy(char *dst, const char *src) {
    char *out = dst;
    while ((*out++ = *src++) != '\0') {}
    return dst;
}

char *strncpy(char *dst, const char *src, size_t n) {
    char *out = dst;
    while (n && (*out = *src) != '\0') { out++; src++; n--; }
    while (n--) { *out++ = '\0'; }
    return dst;
}

char *strchr(const char *s, int c) {
    char ch = (char)c;
    for (;; s++) {
        if (*s == ch) return (char *)s;
        if (*s == '\0') return NULL;
    }
}
