use rust_comms::blockchain::algo_ops::{AlgoOps, AlgoChainConfig};
use rust_comms::blockchain::error::{AlgoError, AlgoErrorKind};

#[tokio::test]
async fn test_node_unreachable() {
    let mut config = AlgoChainConfig::default();
    // Use an unreachable address
    config.client_api_url = "http://localhost".to_string();
    config.client_api_port = 1234;
    
    let (id, _pass) = AlgoOps::generate_keypair();
    let ops = AlgoOps::new(
        None,
        Some(id),
        Some(config),
    );
    
    // account_balance makes a network call
    let result = ops.account_balance();
    
    assert!(result.is_err());
    let err = result.unwrap_err();
    
    let algo_err = err.downcast_ref::<AlgoError>().expect("Should be an AlgoError");
    assert_eq!(algo_err.kind, AlgoErrorKind::HostUnreachable);
    assert_eq!(algo_err.operation, "account_information");
    
    println!("Caught expected error: {}", algo_err);
}
