use std::cell::RefCell;

thread_local!(pub static RESOLV_STATE: RefCell<ResolverState> =
    RefCell::new(ResolverState::init().expect("unable to initialize libresolv state"))
);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResolverError {
    #[error("unknown host")]
    HostNotFound,
    #[error("hostname lookup failure")]
    TryAgain,
    #[error("unknown server error")]
    NoRecovery,
    #[error("no address associated with name")]
    NoData,
    #[error("unexpected h_errno: {0}")]
    Unexpected(std::os::raw::c_int),
}

/// `libresolv` resolver state. The contained state __must not be moved__ since
/// it contains self-referential pointers in `res_state.dnsrch`. If we didn't
/// have to pass raw pointers around in FFI, the state would be `Pin`ned and
/// `res_state` would be wrapped in a `!Unpin` struct.
pub struct ResolverState(Box<res_state>);

impl ResolverState {
    pub fn init() -> Result<Self, ResolverError> {
        let mut state = Self(Box::new(res_state::default()));
        let ret = unsafe { res_ninit(state.as_mut()) };
        if ret >= 0 {
            Ok(state)
        } else {
            Err(ResolverError::Unexpected(ret))
        }
    }

    pub fn check(&self, err: impl PartialOrd<i32>) -> Result<(), ResolverError> {
        if err >= 0 {
            Ok(())
        } else {
            match self.as_ref().res_h_errno {
                0 => Ok(()),
                1 => Err(ResolverError::HostNotFound),
                2 => Err(ResolverError::TryAgain),
                3 => Err(ResolverError::NoRecovery),
                4 => Err(ResolverError::NoData),
                err => Err(ResolverError::Unexpected(err)),
            }
        }
    }
}

impl AsRef<res_state> for ResolverState {
    fn as_ref(&self) -> &res_state {
        &self.0
    }
}

impl AsMut<res_state> for ResolverState {
    fn as_mut(&mut self) -> &mut res_state {
        &mut self.0
    }
}

impl Drop for ResolverState {
    fn drop(&mut self) {
        unsafe { res_nclose(self.as_mut()) };
    }
}

pub fn ns_msg_base(handle: libresolv_sys::ns_msg) -> *const libresolv_sys::u_char {
    handle._msg
}

pub fn ns_msg_end(handle: libresolv_sys::ns_msg) -> *const libresolv_sys::u_char {
    handle._eom
}

pub fn ns_msg_count(
    handle: libresolv_sys::ns_msg,
    section: libresolv_sys::ns_sect,
) -> libresolv_sys::u_int16_t {
    handle._counts[section as usize]
}

pub use libresolv_sys::__dn_expand as dn_expand;
pub use libresolv_sys::__ns_class_ns_c_in as ns_c_in;
pub use libresolv_sys::__ns_sect_ns_s_an as ns_s_an;
pub use libresolv_sys::__ns_type_ns_t_srv as ns_t_srv;
pub use libresolv_sys::__res_nclose as res_nclose;
pub use libresolv_sys::__res_ninit as res_ninit;
pub use libresolv_sys::__res_nsearch as res_nsearch;
pub use libresolv_sys::__res_state as res_state;
pub use libresolv_sys::ns_initparse;
pub use libresolv_sys::ns_msg;
pub use libresolv_sys::ns_parserr;
pub use libresolv_sys::ns_rr;
pub use libresolv_sys::NS_MAXDNAME;
pub use libresolv_sys::NS_MAXMSG;
pub use libresolv_sys::NS_PACKETSZ;
