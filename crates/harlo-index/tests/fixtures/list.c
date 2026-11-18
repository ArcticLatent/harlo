#include <stdio.h>
#include "list.h"

#define MAX 3
#define SQUARE(x) ((x) * (x))

/** A point. */
struct point {
    int x;
};

typedef struct {
    int y;
} pair;

static int add(int a, int b)
{
    return a + b;
}

int *make(void) {
    printf("%d", add(1, 2));
    return NULL;
}
