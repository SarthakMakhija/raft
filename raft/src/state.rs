use std::sync::RwLock;

use crate::replica_role::ReplicaRole;

pub struct State {
    consensus_state: RwLock<ConsensusState>,
}

struct ConsensusState {
    term: u64,
    role: ReplicaRole,
}

impl State {
    pub fn new() -> State {
        return State {
            consensus_state: RwLock::new(ConsensusState {
                term: 0,
                role: ReplicaRole::Follower,
            }),
        };
    }

    pub(crate) fn change_to_follower(&self) -> u64 {
        let mut write_guard = self.consensus_state.write().unwrap();
        let mut consensus_state = &mut *write_guard;
        consensus_state.term = consensus_state.term + 1;
        consensus_state.role = ReplicaRole::Candidate;

        return consensus_state.term;
    }

    pub(crate) fn change_to_leader(&self) {
        let mut write_guard = self.consensus_state.write().unwrap();
        let mut consensus_state = &mut *write_guard;
        consensus_state.role = ReplicaRole::Leader;
    }

    pub fn get_term(&self) -> u64 {
        let guard = self.consensus_state.read().unwrap();
        return (*guard).term;
    }

    pub fn get_role(&self) -> ReplicaRole {
        let guard = self.consensus_state.read().unwrap();
        return (*guard).role;
    }
}