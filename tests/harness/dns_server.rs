//! Minimal mock DNS server for testing SRV record resolution.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use hickory_proto::{
    op::{Message, MessageType, OpCode, ResponseCode},
    rr::{rdata::SRV, Name, RData, Record, RecordType},
    serialize::binary::{BinDecodable, BinEncodable},
};

use crate::harness::MockSrv;

/// A minimal DNS server that responds to SRV queries.
pub struct DnsServer {
    records: Vec<MockSrv>,
    socket: UdpSocket,
    shutdown_handle: ShutdownHandle,
}

impl DnsServer {
    /// Start the server in a background thread.
    pub fn spawn(srv_records: &[MockSrv]) -> io::Result<DnsServerHandle> {
        let output = Command::new("ip")
            .args(["link", "set", "lo", "up"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("failed to bring up loopback interface: {}", stderr);
        }

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 53);
        let socket = UdpSocket::bind(addr)?;
        socket.set_read_timeout(Some(Duration::from_millis(1000)))?;
        let shutdown_handle = ShutdownHandle(Arc::new(AtomicBool::new(false)));
        let this = Self {
            records: srv_records.to_vec(),
            socket,
            shutdown_handle: shutdown_handle.clone(),
        };
        let join_handle = std::thread::spawn(move || this.run());
        println!("mock DNS server started on {addr}");

        Ok(DnsServerHandle {
            shutdown_handle,
            join_handle: Some(join_handle),
        })
    }

    /// Run the server, blocking the current thread.
    /// Returns when shutdown is triggered or an unrecoverable error occurs.
    pub fn run(&self) -> io::Result<()> {
        let mut buf = [0u8; 512];
        while !self.shutdown_handle.is_shutdown() {
            let (len, src) = match self.socket.recv_from(&mut buf) {
                Ok(result) => result,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };

            if let Ok(response) = self.handle_query(&buf[..len]) {
                let _ = self.socket.send_to(&response, src);
            }
        }

        Ok(())
    }

    fn handle_query(&self, query_bytes: &[u8]) -> Result<Vec<u8>, ()> {
        let query = Message::from_bytes(query_bytes).map_err(|_| ())?;
        assert!(
            query
                .queries()
                .iter()
                .all(|q| q.query_type() == RecordType::SRV),
            "expected only SRV queries in the query",
        );

        let mut response = Message::new();
        response.set_id(query.id());
        response.set_message_type(MessageType::Response);
        response.set_op_code(OpCode::Query);
        response.set_authoritative(true);
        response.set_recursion_desired(query.recursion_desired());
        response.set_recursion_available(false);

        for question in query.queries() {
            response.add_query(question.clone());
            let qname = Self::normalize_name(&question.name().to_string());
            let answers = self
                .records
                .iter()
                .filter(|srv| Self::normalize_name(srv.name) == qname)
                .filter_map(|srv| Self::create_srv_record(srv, question.name().clone()).ok());
            response.add_answers(answers);
        }

        if response.answers().is_empty() {
            response.set_response_code(ResponseCode::NXDomain);
        }

        response.to_bytes().map_err(|_| ())
    }

    /// Normalize a DNS name for comparison (lowercase, no trailing dot).
    fn normalize_name(name: &str) -> String {
        name.to_lowercase().trim_end_matches('.').to_string()
    }

    fn create_srv_record(srv: &MockSrv, name: Name) -> Result<Record, ()> {
        let target = Name::from_utf8(srv.target).map_err(|_| ())?;
        let srv_rdata = SRV::new(srv.priority, srv.weight, srv.port, target);
        let record = Record::from_rdata(name, srv.ttl, RData::SRV(srv_rdata));
        Ok(record)
    }
}

/// Handle for the mock DNS server that shuts it down when dropped.
pub struct DnsServerHandle {
    shutdown_handle: ShutdownHandle,
    join_handle: Option<std::thread::JoinHandle<std::io::Result<()>>>,
}

impl Drop for DnsServerHandle {
    fn drop(&mut self) {
        self.shutdown_handle.shutdown();
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Handle for shutting down a running mock DNS server.
#[derive(Clone)]
pub struct ShutdownHandle(Arc<AtomicBool>);

impl ShutdownHandle {
    /// Signal the server to shut down.
    pub fn shutdown(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Returns `true` if a shutdown has been requested.
    fn is_shutdown(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
