#![feature(new_uninit)]

use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use argh::FromArgs;
use std::boxed::Box;
use std::mem::size_of;
use std::mem::MaybeUninit;
use std::net::UdpSocket;
use std::slice;

/// # Safety
/// Implementing this trait is safe only when the target type can be converted
/// mutually between a byte sequence of the same size, which means that no ownership
/// nor memory references are involved.
pub unsafe trait Sliceable: Sized + Copy + Clone {
    fn copy_into_slice(&self) -> Box<[u8]> {
        let mut values = Box::<[u8]>::new_uninit_slice(size_of::<Self>());
        unsafe {
            values.copy_from_slice(slice::from_raw_parts(
                self as *const Self as *const MaybeUninit<u8>,
                size_of::<Self>(),
            ));
            values.assume_init()
        }
    }
    fn copy_from_slice(data: &[u8]) -> Result<Self> {
        if size_of::<Self>() > data.len() {
            Err(anyhow!("data is too short"))
        } else {
            Ok(unsafe { *(data.as_ptr() as *const Self) })
        }
    }
}
unsafe impl Sliceable for PacketHeader {}
unsafe impl Sliceable for StatusRequest {}

#[derive(Debug, FromArgs)]
/// Reach new heights.
struct Args {
    /// an optional nickname for the pilot
    #[argh(positional)]
    device_ip: String,
}

#[repr(packed)]
#[derive(Copy, Clone, Debug)]
struct PacketHeader {
    _signature: [u8; 4],  // "TPRT" for requests, "tprt" for responses
    _const00_be: [u8; 4], // 00 00 00 00
    _const01_be: [u8; 4], // 00 00 00 01
    _const20_be: [u8; 4], // 00 00 00 20
    _cmd_be: [u8; 4],
    _data_size_be: [u8; 4],
    _ip_addr_be: [u8; 4],
    _token_be: [u8; 4],
}
impl PacketHeader {
    fn new_request(cmd: u32, data_size: u32) -> Self {
        Self {
            _signature: *b"TPRT",
            _const00_be: 0x00u32.to_be_bytes(),
            _const01_be: 0x01u32.to_be_bytes(),
            _const20_be: 0x20u32.to_be_bytes(),
            _cmd_be: cmd.to_be_bytes(),
            _data_size_be: data_size.to_be_bytes(),
            _ip_addr_be: 0x00u32.to_be_bytes(),
            _token_be: 0x00u32.to_be_bytes(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum TapeKind {
    W6,
    W9,
    W12,
    W18,
    W24,
    W36,
    UnknownTapeIndex(u8),
}

#[derive(Copy, Clone, Debug)]
enum PrinterStatus {
    NoTape,
    SomeTape(TapeKind),
    CoverIsOpened,
    Unknown(PacketHeader),
}

#[repr(packed)]
#[derive(Copy, Clone)]
struct StatusRequest {
    _header: PacketHeader,
}
impl StatusRequest {
    fn new() -> Self {
        Self {
            _header: PacketHeader::new_request(1, 0),
        }
    }
    fn send(socket: &UdpSocket, device_ip: &str) -> Result<PrinterStatus> {
        let req_status = StatusRequest::new();
        socket
            .send_to(
                &req_status.copy_into_slice(),
                device_ip.to_string() + ":9100",
            )
            .context("failed to send")?;
        println!("sent!");
        let mut buf = [0; 128];
        let (len, src) = socket.recv_from(&mut buf)?;
        println!("recv!");
        let res_header = PacketHeader::copy_from_slice(&buf[0..len])?;
        println!("{} {} {:?}", src, len, &buf[0..len]);
        println!("{:?}", res_header);
        let data = &buf[size_of::<PacketHeader>()..len];
        println!("{:?}", data);
        Ok(match data[0x02] {
            0x06 => PrinterStatus::NoTape,
            0x21 => PrinterStatus::CoverIsOpened,
            0x00 => PrinterStatus::SomeTape(match data[0x03] {
                0x01 => TapeKind::W6,
                0x02 => TapeKind::W9,
                0x03 => TapeKind::W12,
                0x04 => TapeKind::W18,
                0x05 => TapeKind::W24,
                0x06 => TapeKind::W36,
                ti => TapeKind::UnknownTapeIndex(ti),
            }),
            _ => PrinterStatus::Unknown(res_header),
        })
    }
}

fn main() -> Result<()> {
    let args: Args = argh::from_env();
    println!("{:?}", args);

    let socket = UdpSocket::bind("0.0.0.0:0").context("failed to bind")?;
    let info = StatusRequest::send(&socket, &args.device_ip)?;
    println!("{:?}", info);

    Ok(())
}
