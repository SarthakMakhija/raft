use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tokio::runtime::{Builder, Runtime};
use tonic::{Request, Response};

use raft::election::election::Election;
use raft::heartbeat_config::HeartbeatConfig;
use raft::net::rpc::grpc::Command;
use raft::net::rpc::grpc::raft_client::RaftClient;
use raft::net::rpc::grpc::raft_server::RaftServer;
use raft::net::service::raft_service::RaftService;
use raft::state::{ReplicaRole, State};
use replicate::clock::clock::SystemClock;
use replicate::net::connect::error::ServiceResponseError;
use replicate::net::connect::host_and_port::HostAndPort;
use replicate::net::connect::service_registration::{AllServicesShutdownHandle, ServiceRegistration};
use replicate::net::replica::Replica;

#[test]
fn do_not_replicate_log_as_previous_entries_do_not_match() {
    let runtime = Builder::new_multi_thread()
        .thread_name("replicate_log_successfully".to_string())
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    let self_host_and_port = HostAndPort::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 7191);
    let peer_one = HostAndPort::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 7192);
    let peer_other = HostAndPort::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 7193);

    let (all_services_shutdown_handle_one, state) = spin_self(&runtime, self_host_and_port.clone(), vec![peer_one, peer_other]);
    let (all_services_shutdown_handle_two, state_peer_one) = spin_peer(&runtime, peer_one.clone(), vec![self_host_and_port, peer_other]);
    let (all_services_shutdown_handle_three, state_peer_other) = spin_other_peer(&runtime, peer_other.clone(), vec![self_host_and_port, peer_one]);

    let election = Election::new(state.clone());
    election.start();

    thread::sleep(Duration::from_millis(20));

    assert_eq!(ReplicaRole::Leader, state.get_role());
    let content = String::from("replicate");
    let blocking_runtime = Builder::new_current_thread().enable_all().build().unwrap();

    blocking_runtime.block_on(async {
        send_a_command(self_host_and_port, Command {
            command: content.as_bytes().to_vec()
        }).await.unwrap();
    });

    thread::sleep(Duration::from_millis(40));

    blocking_runtime.block_on(async move {
        assert_eq!(1, state.total_log_entries());
        assert_eq!(0, state_peer_one.total_log_entries());
        assert_eq!(0, state_peer_other.total_log_entries());

        all_services_shutdown_handle_one.shutdown().await.unwrap();
        all_services_shutdown_handle_two.shutdown().await.unwrap();
        all_services_shutdown_handle_three.shutdown().await.unwrap();
    });
}

async fn send_a_command(address: HostAndPort, command: Command) -> Result<Response<()>, ServiceResponseError> {
    let mut client = RaftClient::connect(address.as_string_with_http()).await?;
    let response = client.execute(Request::new(command)).await?;
    return Ok(response);
}

fn spin_self(runtime: &Runtime, self_host_and_port: HostAndPort, peers: Vec<HostAndPort>) -> (AllServicesShutdownHandle, Arc<State>) {
    let (all_services_shutdown_handle, all_services_shutdown_receiver) = AllServicesShutdownHandle::new();
    let replica = Replica::new(
        10,
        self_host_and_port.clone(),
        peers,
        Arc::new(SystemClock::new()),
    );

    let state = runtime.block_on(async move {
        return State::new(Arc::new(replica), HeartbeatConfig::default());
    });
    let inner_state = state.clone();
    runtime.spawn(async move {
        ServiceRegistration::register_services_on(
            &self_host_and_port,
            RaftServer::new(RaftService::new(inner_state)),
            all_services_shutdown_receiver,
        ).await;
    });
    (all_services_shutdown_handle, state.clone())
}

fn spin_peer(runtime: &Runtime, self_host_and_port: HostAndPort, peers: Vec<HostAndPort>) -> (AllServicesShutdownHandle, Arc<State>) {
    let (all_services_shutdown_handle, all_services_shutdown_receiver) = AllServicesShutdownHandle::new();
    let replica = Replica::new(
        20,
        self_host_and_port.clone(),
        peers,
        Arc::new(SystemClock::new()),
    );
    let state = runtime.block_on(async move {
        return State::new(Arc::new(replica), HeartbeatConfig::default());
    });
    let inner_state = state.clone();
    runtime.spawn(async move {
        ServiceRegistration::register_services_on(
            &self_host_and_port,
            RaftServer::new(RaftService::new(inner_state)),
            all_services_shutdown_receiver,
        ).await;
    });
    (all_services_shutdown_handle, state.clone())
}

fn spin_other_peer(runtime: &Runtime, self_host_and_port: HostAndPort, peers: Vec<HostAndPort>) -> (AllServicesShutdownHandle, Arc<State>) {
    let (all_services_shutdown_handle, all_services_shutdown_receiver) = AllServicesShutdownHandle::new();
    let replica = Replica::new(
        30,
        self_host_and_port.clone(),
        peers,
        Arc::new(SystemClock::new()),
    );

    let state = runtime.block_on(async move {
        return State::new(Arc::new(replica), HeartbeatConfig::default());
    });
    let inner_state = state.clone();
    runtime.spawn(async move {
        ServiceRegistration::register_services_on(
            &self_host_and_port,
            RaftServer::new(RaftService::new(inner_state)),
            all_services_shutdown_receiver,
        ).await;
    });
    (all_services_shutdown_handle, state.clone())
}