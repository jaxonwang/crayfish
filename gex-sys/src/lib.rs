#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(improper_ctypes)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::default::Default;
use std::env::args;
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::os::raw::*;
use std::ptr::null;

fn uninit<T>() -> MaybeUninit<T> {
    MaybeUninit::<T>::uninit()
}

fn assert_gasnet_ok(ret: c_int) {
    assert_eq!(ret, GASNET_OK as c_int, "GASNet: {}", unsafe {
        CStr::from_ptr(gasnet_ErrorName(ret) as *const c_char)
            .to_str()
            .unwrap()
    });
}

pub fn gex_client_init() -> (Vec<String>, gex_Client_t, gex_EP_t, gex_TM_t) {
    let args = args()
        .map(|arg| CString::new(arg).unwrap())
        .collect::<Vec<CString>>();
    let mut c_args = args
        .into_iter()
        .map(|arg| arg.into_raw())
        .collect::<Vec<*mut c_char>>();
    let mut argc = c_args.len() as c_int;
    let mut ptr: *mut *mut c_char = c_args.as_mut_ptr();
    let mut client = uninit::<gex_Client_t>();
    let mut ep = uninit::<gex_EP_t>();
    let mut tm = uninit::<gex_TM_t>();
    unsafe {
        assert_gasnet_ok(gex_Client_Init_Wrap(
            client.as_mut_ptr(),
            ep.as_mut_ptr(),
            tm.as_mut_ptr(),
            null::<c_char>(),
            &mut argc as *mut c_int,
            &mut ptr as *mut *mut *mut c_char,
            0,
        ));
    }
    let client = unsafe { client.assume_init() };
    let ep = unsafe { ep.assume_init() };
    let tm = unsafe { tm.assume_init() };
    let mut ret = vec![];
    for i in 0..c_args.len() {
        let pos = &mut c_args[i] as *mut *mut c_char;
        if ptr <= pos && pos < unsafe { ptr.offset(argc as isize) } {
            ret.push(
                unsafe { CString::from_raw(c_args[i]) }
                    .into_string()
                    .unwrap(),
            );
        } else {
            drop(unsafe { CString::from_raw(c_args[i]) });
        }
    }
    (ret, client, ep, tm)
}

pub fn gex_token_info(token: gex_Token_t) -> gex_Token_Info_t {
    let mut token_info = uninit::<gex_Token_Info_t>();
    unsafe {
        gex_Token_Info_Wrap(
            token,
            token_info.as_mut_ptr(),
            gex_ti_srcrank() | gex_ti_ep(),
        ); // return bitmap indicating the supported field. ignore.
        token_info.assume_init()
    }
}

pub fn gex_segment_attach(tm: gex_TM_t, length: usize) -> gex_Segment_t {
    let mut seg = uninit::<gex_Segment_t>();
    unsafe {
        assert_gasnet_ok(gex_Segment_Attach_Wrap(seg.as_mut_ptr(), tm, length));
        seg.assume_init()
    }
}
