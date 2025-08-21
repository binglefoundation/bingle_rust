#ifndef RUST_COMMS_H
#define RUST_COMMS_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Adds two unsigned 64-bit integers. Provided by the Rust static library.
uint64_t rust_comms_add_u64(uint64_t a, uint64_t b);

// Test runners (return 1 on success, 0 on failure)
uint8_t rust_comms_run_algo_ops_tests(void);
uint8_t rust_comms_run_algo_ops_more_tests(void);
uint8_t rust_comms_run_asset_ops_tests(void);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // RUST_COMMS_H
