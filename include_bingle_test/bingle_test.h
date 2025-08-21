#ifndef BINGLE_TEST_H
#define BINGLE_TEST_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Test runners (return 1 on success, 0 on failure)
uint8_t rust_comms_run_algo_ops_tests(void);
uint8_t rust_comms_run_algo_ops_more_tests(void);
uint8_t rust_comms_run_asset_ops_tests(void);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // BINGLE_TEST_H
