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

/// A minimal mock DNS server that responds to SRV queries.
pub struct MockDns {
    records: Vec<MockSrv>,
}

impl MockDns {
    /// Address to bind the DNS server to.
    pub const BIND_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 53);

    /// Files needed for sandboxed DNS resolution via loopback.
    pub const fn config_files() -> &'static [(&'static str, &'static [u8])] {
        &[
            ("/etc/resolv.conf", b"nameserver 127.0.0.1\n"),
            ("/etc/hosts", b"127.0.0.1 localhost\n"),
            ("/etc/nsswitch.conf", b"hosts: files dns\n"),
        ]
    }

    /// Create a DNS server with the given SRV records.
    pub fn new(records: &[MockSrv]) -> Self {
        Self {
            records: records.to_vec(),
        }
    }

    /// Start the server in a background thread.
    /// Brings up the loopback interface, binds to [`Self::BIND_ADDR`], and spawns a
    /// thread that answers SRV queries with the configured records.
    pub fn spawn(&self) -> io::Result<DnsServerHandle> {
        let output = Command::new("ip")
            .args(["link", "set", "lo", "up"])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("failed to bring up loopback interface: {}", stderr);
        }

        let socket = UdpSocket::bind(Self::BIND_ADDR)?;
        socket.set_read_timeout(Some(Duration::from_millis(1000)))?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);
        let records = self.records.clone();
        let join_handle = std::thread::spawn(move || Self::run(&records, &socket, &shutdown_clone));

        Ok(DnsServerHandle {
            shutdown,
            join_handle: Some(join_handle),
        })
    }

    /// Run the server loop, blocking the current thread.
    /// Returns when shutdown is triggered or an unrecoverable error occurs.
    fn run(records: &[MockSrv], socket: &UdpSocket, shutdown: &AtomicBool) -> io::Result<()> {
        let mut buf = [0u8; 512];
        while !shutdown.load(Ordering::Relaxed) {
            let (len, src) = match socket.recv_from(&mut buf) {
                Ok(result) => result,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };

            if let Ok(response) = Self::handle_query(records, &buf[..len]) {
                let _ = socket.send_to(&response, src);
            }
        }

        Ok(())
    }

    fn handle_query(records: &[MockSrv], query_bytes: &[u8]) -> Result<Vec<u8>, ()> {
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
            let answers = records
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

/// Static SRV record definition for use in test configurations.
#[derive(Clone, Debug)]
pub struct MockSrv {
    /// The SRV name (e.g., `_http._tcp.example.com`)
    pub name: &'static str,
    /// Priority value
    pub priority: u16,
    /// Weight value
    pub weight: u16,
    /// Port number
    pub port: u16,
    /// Target hostname
    pub target: &'static str,
    /// TTL in seconds
    pub ttl: u32,
}

impl MockSrv {
    /// Create a new SRV record.
    pub const fn new(
        name: &'static str,
        priority: u16,
        weight: u16,
        port: u16,
        target: &'static str,
        ttl: u32,
    ) -> Self {
        Self {
            name,
            priority,
            weight,
            port,
            target,
            ttl,
        }
    }
}

/// Handle for the mock DNS server that shuts it down when dropped.
pub struct DnsServerHandle {
    shutdown: Arc<AtomicBool>,
    join_handle: Option<std::thread::JoinHandle<std::io::Result<()>>>,
}

impl Drop for DnsServerHandle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}
