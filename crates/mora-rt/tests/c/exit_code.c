/* `cargo xtask runtime --test` links this against each runtime variant: the
   process must exit with the code `__mora_main` returns. */

#include <stdint.h>

int32_t __mora_main(void) {
    return 42;
}
