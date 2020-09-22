//! SRV Resolver backed by `libresolv`.

use super::{SrvRecord, SrvResolver};
use async_trait::async_trait;
use std::{convert::TryInto, ffi::CString, time::Duration};

mod ffi;

/// Errors encountered by [`LibResolv`].
///
/// [`LibResolv`]: struct.LibResolv.html
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LibResolvError {
    /// Rust -> C string conversion errors.
    #[error("srv name contained interior null byte: {0}")]
    InteriorNul(#[from] std::ffi::NulError),
    /// SRV resolver errors.
    #[error("resolver: {0}")]
    Resolver(#[from] ffi::ResolverError),
    /// Tried to parse non-SRV record as SRV.
    #[error("record type is not SRV")]
    NotSrv,
    /// DNS answer larger than allowed by RFC.
    #[error("DNS answer did not fit in maximum message size (65535)")]
    AnswerTooLarge,
}

/// SRV Resolver backed by `libresolv`.
#[derive(Debug)]
pub struct LibResolv {
    initial_buf_size: usize,
}

impl LibResolv {
    /// Initialzes a resolver with a specific initial buffer size for DNS answers.
    pub fn new(initial_buf_size: usize) -> Self {
        Self { initial_buf_size }
    }
}

impl Default for LibResolv {
    fn default() -> Self {
        Self::new(ffi::NS_PACKETSZ as usize)
    }
}

#[async_trait]
impl SrvResolver for LibResolv {
    type Record = LibResolvSrvRecord;
    type Error = LibResolvError;

    async fn get_srv_records_unordered(&self, srv: &str) -> Result<Vec<Self::Record>, Self::Error> {
        let srv = CString::new(srv)?;
        let mut buf = vec![0u8; self.initial_buf_size];
        ffi::RESOLV_STATE.with(|state| {
            let mut state = state.borrow_mut();
            let len = loop {
                let len = unsafe {
                    ffi::res_nquery(
                        state.as_mut(),
                        srv.as_ptr(),
                        ffi::ns_c_any as i32,
                        ffi::ns_t_srv as i32,
                        buf.as_mut_ptr(),
                        buf.len() as i32,
                    )
                };
                // Retry with larger buffer on TRY_AGAIN or `len` larger than buffer
                let len = match state.check(len) {
                    Ok(()) => len as usize,
                    Err(ffi::ResolverError::TryAgain) => buf.len() * 2,
                    Err(e) => return Err(e.into()),
                };
                if len <= buf.len() {
                    break len;
                } else if len <= ffi::NS_MAXMSG as usize {
                    buf.resize(len, 0)
                } else {
                    return Err(LibResolvError::AnswerTooLarge);
                }
            };

            let response = &buf[..len];
            let mut msg = unsafe { std::mem::zeroed() };
            let ret =
                unsafe { ffi::ns_initparse(response.as_ptr(), response.len() as i32, &mut msg) };
            state.check(ret)?;

            let mut rr = unsafe { std::mem::zeroed() };
            let records = (0..ffi::ns_msg_count(msg, ffi::ns_s_an))
                .map(|idx| {
                    let ret =
                        unsafe { ffi::ns_parserr(&mut msg, ffi::ns_s_an, idx as i32, &mut rr) };
                    state.check(ret)?;
                    LibResolvSrvRecord::try_parse(&state, msg, rr)
                })
                .collect::<Result<Vec<_>, _>>()?;

            Ok(records)
        })
    }
}

/// Representation of SRV records used by [`LibResolv`].
///
/// [`LibResolv`]: struct.LibResolv.html
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibResolvSrvRecord {
    /// Record's time-to-live.
    pub ttl: Duration,
    /// Records's target.
    pub target: String,
    /// Record's port.
    pub port: u16,
    /// Record's priority.
    pub priority: u16,
    /// Record's weight.
    pub weight: u16,
}

impl SrvRecord for LibResolvSrvRecord {
    fn ttl(&self) -> Duration {
        self.ttl
    }

    fn target(&self) -> &str {
        &self.target
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn priority(&self) -> u16 {
        self.priority
    }

    fn weight(&self) -> u16 {
        self.weight
    }
}

impl LibResolvSrvRecord {
    fn try_parse(
        state: &ffi::ResolverState,
        msg: ffi::ns_msg,
        rr: ffi::ns_rr,
    ) -> Result<Self, LibResolvError> {
        if rr.type_ as u32 != ffi::ns_t_srv {
            return Err(LibResolvError::NotSrv);
        }

        let (header, rest) =
            unsafe { std::slice::from_raw_parts(rr.rdata, rr.rdlength as usize) }.split_at(6);

        let mut chunks = header
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes(chunk.try_into().unwrap()));

        let priority = chunks.next().unwrap();
        let weight = chunks.next().unwrap();
        let port = chunks.next().unwrap();

        let mut name = [0u8; ffi::NS_MAXDNAME as usize];
        let ret = unsafe {
            ffi::dn_expand(
                ffi::ns_msg_base(msg),
                ffi::ns_msg_end(msg),
                rest.as_ptr(),
                name.as_mut_ptr().cast(),
                name.len() as i32,
            )
        };
        state.check(ret)?;

        let target = unsafe { std::ffi::CStr::from_ptr(name.as_ptr().cast()) };
        Ok(Self {
            ttl: Duration::from_secs(rr.ttl as u64),
            target: target.to_string_lossy().to_string(),
            port,
            priority,
            weight,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_srv_lookup() -> Result<(), LibResolvError> {
        let records = LibResolv::default()
            .get_srv_records_unordered("_http._tcp.srv-client-rust.deshaw.org")
            .await?;
        assert_ne!(records.len(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_srv_lookup_ordered() -> Result<(), LibResolvError> {
        let records = LibResolv::default()
            .get_srv_records("_http._tcp.srv-client-rust.deshaw.org")
            .await?;
        assert_ne!(records.len(), 0);
        assert!((0..records.len() - 1).all(|i| records[i].priority() <= records[i + 1].priority()));
        Ok(())
    }

    #[tokio::test]
    async fn invalid_host() {
        assert_eq!(
            LibResolv::default()
                .get_srv_records("_http._tcp.foobar.deshaw.com")
                .await,
            Err(ffi::ResolverError::HostNotFound.into())
        );
    }

    #[tokio::test]
    async fn malformed_srv_name() {
        assert_eq!(
            LibResolv::default()
                .get_srv_records("_http.foobar.deshaw.com")
                .await,
            Err(ffi::ResolverError::HostNotFound.into())
        );
    }

    #[tokio::test]
    async fn very_malformed_srv_name() {
        assert_eq!(
            LibResolv::default()
                .get_srv_records("  @#*^[_hsd flt.com")
                .await,
            Err(ffi::ResolverError::HostNotFound.into())
        );
    }

    #[tokio::test]
    async fn srv_name_containing_nul_terminator() {
        assert!(matches!(
            LibResolv::default()
                .get_srv_records("_http.\0_tcp.foo.com")
                .await,
            Err(LibResolvError::InteriorNul(_))
        ));
    }
}
