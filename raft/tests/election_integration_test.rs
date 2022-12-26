use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};
use raft::election::election::Election;
use raft::net::service::voting_service::VotingService;
use raft::state::State;
use replicate::clock::clock::SystemClock;
use replicate::net::connect::host_and_port::HostAndPort;
use replicate::net::connect::service_registration::{AllServicesShutdownHandle, ServiceRegistration};
use replicate::net::replica::Replica;
use raft::net::rpc::grpc::voting_server::VotingServer;
use raft::replica_role::ReplicaRole;

#[test]
fn start_elections_with_new_term() {
    let runtime = Builder::new_multi_thread()
        .thread_name("start_election".to_string())
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    let self_host_and_port = HostAndPort::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3560);
    let peer_one = HostAndPort::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3561);
    let peer_other = HostAndPort::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3562);

    let state = Arc::new(State::new());

    let (all_services_shutdown_handle_one, replica_self) = spin_self(&runtime, self_host_and_port.clone(), vec![peer_one, peer_other], state.clone());
    let all_services_shutdown_handle_two = spin_peer(&runtime, peer_one.clone(), vec![self_host_and_port, peer_other], Arc::new(State::new()));
    let all_services_shutdown_handle_three = spin_other_peer(&runtime, peer_other.clone(), vec![self_host_and_port, peer_one], Arc::new(State::new()));

    let_services_start();

    let election = Election::new(
        state.clone(),
        replica_self.clone()
    );
    election.start();
    thread::sleep(Duration::from_secs(1));
    assert_eq!(1, state.get_term());

    election.start();
    thread::sleep(Duration::from_secs(1));
    assert_eq!(2, state.get_term());
    assert_eq!(ReplicaRole::Leader, state.get_role());

    let blocking_runtime = Builder::new_current_thread().enable_all().build().unwrap();
    blocking_runtime.block_on(async move {
        all_services_shutdown_handle_one.shutdown().await.unwrap();
        all_services_shutdown_handle_two.shutdown().await.unwrap();
        all_services_shutdown_handle_three.shutdown().await.unwrap();
    });
}

fn spin_self(runtime: &Runtime, self_host_and_port: HostAndPort, peers: Vec<HostAndPort>, state: Arc<State>) -> (AllServicesShutdownHandle, Arc<Replica>) {
    let (all_services_shutdown_handle, all_services_shutdown_receiver) = AllServicesShutdownHandle::new();
    let replica = Replica::new(
        String::from("mark"),
        self_host_and_port.clone(),
        peers,
        Arc::new(SystemClock::new()),
    );

    let replica = Arc::new(replica);
    let cloned = replica.clone();
    runtime.spawn(async move {
        ServiceRegistration::register_services_on(
            &self_host_and_port,
            VotingServer::new(VotingService::new(state, replica.clone())),
            all_services_shutdown_receiver,
        ).await;
    });
    (all_services_shutdown_handle, cloned)
}

fn spin_peer(runtime: &Runtime, self_host_and_port: HostAndPort, peers: Vec<HostAndPort>, state: Arc<State>) -> AllServicesShutdownHandle {
    let (all_services_shutdown_handle, all_services_shutdown_receiver) = AllServicesShutdownHandle::new();
    let replica = Replica::new(
        String::from("mathew"),
        self_host_and_port.clone(),
        peers,
        Arc::new(SystemClock::new()),
    );

    runtime.spawn(async move {
        ServiceRegistration::register_services_on(
            &self_host_and_port,
            VotingServer::new(VotingService::new(state, Arc::new(replica))),
            all_services_shutdown_receiver,
        ).await;
    });
    all_services_shutdown_handle
}

fn spin_other_peer(runtime: &Runtime, self_host_and_port: HostAndPort, peers: Vec<HostAndPort>, state: Arc<State>) -> AllServicesShutdownHandle {
    let (all_services_shutdown_handle, all_services_shutdown_receiver) = AllServicesShutdownHandle::new();
    let replica = Replica::new(
        String::from("john"),
        self_host_and_port.clone(),
        peers,
        Arc::new(SystemClock::new()),
    );

    runtime.spawn(async move {
        ServiceRegistration::register_services_on(
            &self_host_and_port,
            VotingServer::new(VotingService::new(state, Arc::new(replica))),
            all_services_shutdown_receiver,
        ).await;
    });
    all_services_shutdown_handle
}

fn let_services_start() {
    thread::sleep(Duration::from_secs(4));
}