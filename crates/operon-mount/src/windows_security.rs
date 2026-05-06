#![cfg(windows)]

use windows_sys::Win32::{
    Foundation::{LocalFree, STATUS_BUFFER_OVERFLOW, STATUS_SUCCESS},
    Security::{
        Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION},
        GetSecurityDescriptorLength,
    },
};
use winfsp_wrs::{U16CStr, NTSTATUS};
use winfsp_wrs_sys::{PSECURITY_DESCRIPTOR, SIZE_T};

pub(crate) struct WindowsSecurityDescriptor {
    bytes: Vec<u8>,
}

impl WindowsSecurityDescriptor {
    pub(crate) fn from_sddl(sddl: &U16CStr) -> anyhow::Result<Self> {
        let mut descriptor = std::ptr::null_mut();
        let mut reported_len = 0;
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION,
                &mut descriptor,
                &mut reported_len,
            )
        };
        if ok == 0 {
            anyhow::bail!("failed to create Windows security descriptor");
        }

        let len = unsafe { GetSecurityDescriptorLength(descriptor) as usize };
        let mut bytes = vec![0; len];
        unsafe {
            std::ptr::copy_nonoverlapping(descriptor.cast::<u8>(), bytes.as_mut_ptr(), len);
            LocalFree(descriptor.cast());
        }
        Ok(Self { bytes })
    }

    pub(crate) unsafe fn copy_to(
        &self,
        security_descriptor: PSECURITY_DESCRIPTOR,
        security_descriptor_size: *mut SIZE_T,
    ) -> NTSTATUS {
        if security_descriptor_size.is_null() {
            return STATUS_SUCCESS;
        }

        let descriptor_len = self.bytes.len() as SIZE_T;
        if descriptor_len > security_descriptor_size.read() {
            security_descriptor_size.write(descriptor_len);
            return STATUS_BUFFER_OVERFLOW;
        }

        security_descriptor_size.write(descriptor_len);
        if !security_descriptor.is_null() {
            std::ptr::copy_nonoverlapping(
                self.bytes.as_ptr(),
                security_descriptor.cast::<u8>(),
                self.bytes.len(),
            );
        }
        STATUS_SUCCESS
    }
}
