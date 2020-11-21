//! SRV Resolver backed by `libresolv`.

use super::{SrvRecord, SrvResolver};
use async_trait::async_trait;
use std::{
    convert::TryInto,
    ffi::CString,
    time::{Duration, Instant},
};

mod ffi;

/// Errors encountered by [`LibResolv`].
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

    async fn get_srv_records_unordered(
        &self,
        srv: &str,
    ) -> Result<(Vec<Self::Record>, Instant), Self::Error> {
        let srv = CString::new(srv)?;
        let mut buf = vec![0u8; self.initial_buf_size];
        ffi::RESOLV_STATE.with(|state| {
            let mut state = state.borrow_mut();
            let (len, response_time) = loop {
                let len = unsafe {
                    ffi::res_nsearch(
                        state.as_mut(),
                        srv.as_ptr(),
                        ffi::ns_c_in as i32,
                        ffi::ns_t_srv as i32,
                        buf.as_mut_ptr(),
                        buf.len() as i32,
                    )
                };
                let len = match state.check(len) {
                    Ok(()) => len as usize,
                    Err(e) => return Err(e.into()),
                };
                if len <= buf.len() {
                    break (len, Instant::now());
                } else if len <= ffi::NS_MAXMSG as usize {
                    // Retry with larger buffer
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
            let num_records = ffi::ns_msg_count(msg, ffi::ns_s_an);
            let mut records = Vec::with_capacity(num_records as usize);
            let mut min_ttl = None;
            for idx in 0..num_records {
                let ret = unsafe { ffi::ns_parserr(&mut msg, ffi::ns_s_an, idx as i32, &mut rr) };
                state.check(ret)?;
                let (record, ttl) = LibResolvSrvRecord::try_parse(&state, msg, rr)?;
                records.push(record);
                min_ttl = min_ttl.min(Some(ttl)).or(Some(ttl));
            }

            Ok((records, response_time + min_ttl.unwrap_or_default()))
        })
    }
}

/// Representation of SRV records used by [`LibResolv`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibResolvSrvRecord {
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
    type Target = str;

    fn target(&self) -> &Self::Target {
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
    ) -> Result<(Self, Duration), LibResolvError> {
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
        let ttl = Duration::from_secs(rr.ttl as u64);
        let record = Self {
            target: target.to_string_lossy().to_string(),
            port,
            priority,
            weight,
        };
        Ok((record, ttl))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn srv_lookup() -> Result<(), LibResolvError> {
        let (records, valid_until) = LibResolv::default()
            .get_srv_records_unordered(crate::EXAMPLE_SRV)
            .await?;
        assert_ne!(records.len(), 0);
        assert!(valid_until > Instant::now());
        Ok(())
    }

    #[tokio::test]
    async fn srv_lookup_ordered() -> Result<(), LibResolvError> {
        let (records, _) = LibResolv::default()
            .get_srv_records(crate::EXAMPLE_SRV)
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
