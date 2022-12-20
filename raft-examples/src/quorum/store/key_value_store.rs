use std::sync::Arc;

use dashmap::DashMap;
use dashmap::mapref::one::Ref;
use tonic::{Request, Response, Status};

use raft::net::connect::async_network::AsyncNetwork;
use raft::net::connect::headers::{get_referral_host_from, get_referral_port_from};
use raft::net::connect::host_and_port::HostAndPort;
use raft::net::replica::Replica;

use crate::quorum::factory::ServiceRequestFactory;
use crate::quorum::rpc::grpc::CorrelatingGetValueByKeyRequest;
use crate::quorum::rpc::grpc::GetValueByKeyResponse;
use crate::quorum::rpc::grpc::PutKeyValueResponse;
use crate::quorum::rpc::grpc::VersionedPutKeyValueRequest;
use crate::quorum::value::Value;

pub(crate) struct KeyValueStore {
    replica: Arc<Replica>,
    storage: Arc<DashMap<String, Value>>,
}

impl KeyValueStore {
    pub(crate) fn set_initial_state(&self, key_value: (String, Value)) {
        self.storage.clone().insert(key_value.0, key_value.1);
    }

    pub(crate) async fn acknowledge_get(&self, request: Request<CorrelatingGetValueByKeyRequest>) -> Result<Response<()>, Status> {
        let optional_host = get_referral_host_from(&request);
        let optional_port = get_referral_port_from(&request);
        if optional_host.is_none() || optional_port.is_none() {
            return Err(Status::failed_precondition(format!("Missing originating host/port in acknowledge_get")));
        }

        let request = request.into_inner();
        println!("Received a correlating get request for key {}", request.key.clone());


        let originating_host_port = HostAndPort::try_new(
            optional_host.unwrap(),
            u16::try_from(optional_port.unwrap()).unwrap(),
        );

        let key = request.key;
        let correlation_id = request.correlation_id;
        let storage = self.storage.clone();
        let source_address = self.replica.clone().get_self_address();

        let handler = async move {
            let value: Option<Ref<String, Value>> = storage.get(&key);
            let response = match value {
                None => {
                    GetValueByKeyResponse { key, value: "".to_string(), correlation_id, timestamp: 0 }
                }
                Some(value_ref) => {
                    let value = value_ref.value();
                    GetValueByKeyResponse { key, value: String::from(value.get_value()), correlation_id, timestamp: value.get_timestamp() }
                }
            };
            AsyncNetwork::send_with_source_footprint(
                ServiceRequestFactory::get_value_by_key_response(correlation_id, response),
                source_address,
                originating_host_port.unwrap(),
            ).await.unwrap();
        };
        let _ = &self.replica.add_to_queue(handler);
        return Ok(Response::new(()));
    }

    pub(crate) async fn finish_get(&self, request: Request<GetValueByKeyResponse>) -> Result<Response<()>, Status> {
        let optional_host = get_referral_host_from(&request);
        let optional_port = get_referral_port_from(&request);
        if optional_host.is_none() || optional_port.is_none() {
            return Err(Status::failed_precondition(format!("Missing originating host/port in finish_get")));
        }

        let originating_host_port = HostAndPort::try_new(
            optional_host.unwrap(),
            u16::try_from(optional_port.unwrap()).unwrap(),
        );

        let response = request.into_inner();
        println!("Received a response for key {}", response.key.clone());

        let _ = &self.replica.register_response(response.correlation_id, originating_host_port.unwrap(), Ok(Box::new(response)));
        return Ok(Response::new(()));
    }

    pub(crate) async fn acknowledge_put(&self, request: Request<VersionedPutKeyValueRequest>) -> Result<Response<()>, Status> {
        let optional_host = get_referral_host_from(&request);
        let optional_port = get_referral_port_from(&request);
        if optional_host.is_none() || optional_port.is_none() {
            return Err(Status::failed_precondition(format!("Missing originating host/port in acknowledge_set")));
        }

        let request = request.into_inner();
        println!("Received a versioned put request for key {} with timestamp {}", request.key.clone(), request.timestamp);

        let originating_host_port = HostAndPort::try_new(
            optional_host.unwrap(),
            optional_port.unwrap(),
        );

        let correlation_id = request.correlation_id;
        let storage = self.storage.clone();
        let source_address = self.replica.clone().get_self_address();

        let handler = async move {
            let key = request.key.clone();
            storage.insert(key, Value::new(request.value.clone(), request.timestamp));

            AsyncNetwork::send_with_source_footprint(
                ServiceRequestFactory::put_key_value_response(
                    correlation_id
                ),
                source_address,
                originating_host_port.unwrap(),
            ).await.unwrap();
        };
        let _ = &self.replica.add_to_queue(handler);
        return Ok(Response::new(()));
    }

    pub(crate) async fn finish_put(&self, request: Request<PutKeyValueResponse>) -> Result<Response<()>, Status> {
        let optional_host = get_referral_host_from(&request);
        let optional_port = get_referral_port_from(&request);
        if optional_host.is_none() || optional_port.is_none() {
            return Err(Status::failed_precondition(format!("Missing originating host/port in finish_put")));
        }
        let originating_host_port = HostAndPort::try_new(
            optional_host.unwrap(),
            u16::try_from(optional_port.unwrap()).unwrap(),
        );

        let response = request.into_inner();
        println!("Received a put response {}", response.was_put);

        let _ = &self.replica.register_response(response.correlation_id, originating_host_port.unwrap(), Ok(Box::new(response)));
        return Ok(Response::new(()));
    }
}

impl KeyValueStore {
    pub(crate) fn new(replica: Arc<Replica>) -> KeyValueStore {
        return KeyValueStore {
            replica,
            storage: Arc::new(DashMap::new()),
        };
    }
}