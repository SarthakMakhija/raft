use std::net::{IpAddr, Ipv4Addr};
use std::thread;
use std::time::Duration;
use replicate::net::connect::async_network::AsyncNetwork;

use replicate::net::connect::host_and_port::HostAndPort;
use replicate::net::connect::random_correlation_id_generator::RandomCorrelationIdGenerator;
use replicate::net::connect::service::heartbeat::service_request::HeartbeatServiceRequest;
use replicate::net::connect::service_client::ServiceResponseError;
use replicate::net::connect::service_registration::{AllServicesShutdownHandle, ServiceRegistration};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send() {
    let server_address = HostAndPort::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 50051);
    let server_address_clone_one = server_address.clone();
    let server_address_clone_other = server_address.clone();

    let (all_services_shutdown_handle, all_services_shutdown_receiver) = AllServicesShutdownHandle::new();
    let server_handle = tokio::spawn(async move {
        ServiceRegistration::register_default_services_on(&server_address_clone_one, all_services_shutdown_receiver).await;
    });

    thread::sleep(Duration::from_secs(3));
    let client_handle = tokio::spawn(async move {
        let response = send_client_request(server_address_clone_other).await;
        assert!(response.is_ok());
        all_services_shutdown_handle.shutdown().await.unwrap();
    });
    server_handle.await.unwrap();
    client_handle.await.unwrap();
}

async fn send_client_request(target_address: HostAndPort) -> Result<(), ServiceResponseError> {
    let node_id = "mark";
    let service_request = HeartbeatServiceRequest::new(
        node_id.to_string(),
        &RandomCorrelationIdGenerator::new()
    );
    return AsyncNetwork::send_without_source_footprint(service_request, target_address).await;
}