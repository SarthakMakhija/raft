use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use dashmap::DashMap;
use dashmap::mapref::one::Ref;
use tonic::{Response, Status};

use raft::net::connect::async_network::AsyncNetwork;
use raft::net::connect::host_and_port::HostAndPort;
use raft::net::connect::service_client::ServiceRequest;
use raft::net::replica::Replica;

use crate::quorum::client_provider::GetValueByKeyResponseClient;
use crate::quorum::rpc::grpc::{GetValueByKeyResponse, VersionedGetValueByKeyRequest};

pub(crate) struct Server {
    replica: Arc<Replica>,
    storage: Arc<DashMap<String, String>>,
}

impl Server {
    pub(crate) async fn acknowledge(&self, request: VersionedGetValueByKeyRequest) -> Result<Response<()>, Status> {
        println!("Received a versioned get request for key {}", request.key.clone());

        let key = request.key;
        let correlation_id = request.correlation_id;
        let storage = self.storage.clone();

        let handler = async move {
            let value: Option<Ref<String, String>> = storage.get(&key);
            let response = match value {
                None =>
                    GetValueByKeyResponse { key, value: "".to_string(), correlation_id },
                Some(value_ref) =>
                    GetValueByKeyResponse { key, value: String::from(value_ref.value()), correlation_id },
            };
            let service_request = ServiceRequest::new(
                response,
                Box::new(GetValueByKeyResponseClient {}),
                correlation_id,
            );
            AsyncNetwork::send(
                service_request,
                HostAndPort::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9090), //TODO: Change the ip address and port
            ).await.unwrap();
        };
        let _ = &self.replica.add_to_queue(handler);
        return Ok(Response::new(()));
    }

    pub(crate) async fn accept(&self, request: GetValueByKeyResponse) -> Result<Response<()>, Status> {
        println!("Received a response for key {}", request.key.clone());

        let _ = &self.replica.register_response(request.correlation_id, Ok(Box::new(request)));
        return Ok(Response::new(()));
    }
}

impl Server {
    pub(crate) fn new(replica: Arc<Replica>) -> Server {
        return Server {
            replica,
            storage: Arc::new(DashMap::new()),
        };
    }
}