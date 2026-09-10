/*
 * Minimal libc string shims for bare-metal builds.
 *
 * thumbv8m.main-none-eabihf links without newlib, so the FreeRTOS+TCP
 * DNS / sockets paths and cyw43-driver reference string functions that
 * aren't provided by compiler_builtins. Keep the implementations small
 * and obvious — correctness matters more than speed at these call sites
 * (DHCP hostname parsing, event-name table lookups).
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
