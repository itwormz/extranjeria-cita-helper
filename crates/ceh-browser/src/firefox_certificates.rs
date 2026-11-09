//! Cache one public intermediate, never a trust anchor or a browser TLS exception.
//! NSS comes from the same installed Firefox; only a CEH-owned, leased profile is opened.
use crate::firefox::{FirefoxError, FirefoxInstallation, ProfileLease};
const INTERMEDIATE: &[u8] = include_bytes!("../assets/rapidssl-tls-rsa-ca-g1.der");
#[cfg(target_os = "macos")]
static NSS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Default)]
struct CertTrust {
    ssl: u32,
    email: u32,
    object_signing: u32,
}
#[cfg(target_os = "macos")]
pub(crate) fn prepare(
    profile: &ProfileLease,
    installation: &FirefoxInstallation,
) -> Result<usize, FirefoxError> {
    use std::{
        ffi::{CString, c_char, c_int, c_ulong, c_void},
        os::unix::ffi::OsStrExt,
    };
    let _lock = NSS_LOCK.lock().map_err(|_| FirefoxError::Unavailable)?;
    let folder = installation
        .executable()
        .parent()
        .ok_or(FirefoxError::InvalidInstallation)?;
    let profile_name = CString::new([b"sql:", profile.path().as_os_str().as_bytes()].concat())
        .map_err(|_| FirefoxError::InvalidProfile)?;
    // Library names/locations belong to the validated installation, never to a UI path.
    unsafe {
        let glue = libloading::os::unix::Library::open(
            Some(folder.join("libmozglue.dylib")),
            libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_GLOBAL,
        )
        .map_err(|_| FirefoxError::Unavailable)?;
        let nss = libloading::Library::new(folder.join("libnss3.dylib"))
            .map_err(|_| FirefoxError::Unavailable)?;
        let init: libloading::Symbol<unsafe extern "C" fn(*const c_char) -> c_int> = nss
            .get(b"NSS_InitReadWrite\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        let shutdown: libloading::Symbol<unsafe extern "C" fn() -> c_int> = nss
            .get(b"NSS_Shutdown\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        let get_slot: libloading::Symbol<unsafe extern "C" fn() -> *mut c_void> = nss
            .get(b"PK11_GetInternalKeySlot\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        let free_slot: libloading::Symbol<unsafe extern "C" fn(*mut c_void)> = nss
            .get(b"PK11_FreeSlot\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        let decode: libloading::Symbol<unsafe extern "C" fn(*const c_char, c_int) -> *mut c_void> =
            nss.get(b"CERT_DecodeCertFromPackage\0")
                .map_err(|_| FirefoxError::Unavailable)?;
        let destroy: libloading::Symbol<unsafe extern "C" fn(*mut c_void)> = nss
            .get(b"CERT_DestroyCertificate\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        let import: libloading::Symbol<
            unsafe extern "C" fn(*mut c_void, *mut c_void, c_ulong, *const c_char, c_int) -> c_int,
        > = nss
            .get(b"PK11_ImportCert\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        let get_trust: libloading::Symbol<
            unsafe extern "C" fn(*mut c_void, *mut CertTrust) -> c_int,
        > = nss
            .get(b"CERT_GetCertTrust\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        if init(profile_name.as_ptr()) != 0 {
            return Err(FirefoxError::InvalidProfile);
        }
        let slot = get_slot();
        let result = if slot.is_null() {
            Err(FirefoxError::Unavailable)
        } else {
            let certificate = decode(INTERMEDIATE.as_ptr().cast(), INTERMEDIATE.len() as c_int);
            let result = if certificate.is_null() {
                Err(FirefoxError::Unavailable)
            } else {
                let nickname = c"ceh-intermediate-rapidssl-tls-rsa-ca-g1";
                let status = import(slot, certificate, 0, nickname.as_ptr(), 0); // includeTrust=false
                let mut trust = CertTrust::default();
                let trust_status = get_trust(certificate, &mut trust);
                // Missing trust is expected; if a record exists, no anchor/peer trust bits are allowed.
                let forbidden = 0x02 | 0x10 | 0x20 | 0x80;
                let result = if status == 0
                    && (trust_status != 0
                        || ((trust.ssl | trust.email | trust.object_signing) & forbidden) == 0)
                {
                    Ok(1)
                } else {
                    Err(FirefoxError::Unavailable)
                };
                destroy(certificate);
                result
            };
            free_slot(slot);
            result
        };
        let closed = shutdown();
        drop(nss);
        drop(glue);
        if closed != 0 {
            return Err(FirefoxError::Unavailable);
        }
        result
    }
}
#[cfg(not(target_os = "macos"))]
pub(crate) fn prepare(
    _profile: &ProfileLease,
    _installation: &FirefoxInstallation,
) -> Result<usize, FirefoxError> {
    let _ = INTERMEDIATE;
    Ok(0)
}
