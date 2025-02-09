use super::SkeletalInputActionStates;
use crate::openxr_data::Hand;
use ipc_channel::ipc::{IpcError, IpcOneShotServer, IpcReceiver, IpcSender};
use libc::{c_void, dladdr, Dl_info};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::ffi::CStr;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
};

/// To ensure skeletal input action sets don't interfere with ones created by the game a separate XrInstance is created,
/// however the OpenXR loader only supports one XrInstance per process, so we create a separate process that reports back with the data over IPC.
///
/// XRizer gets built as both a cdylib and regular lib that gets statically linked into the binary, allowing it to share interaction profiles and other code.
pub struct SkeletalInputIPC {
    connection: Option<IPCConnection>,
}

impl SkeletalInputIPC {
    #[allow(unreachable_code)]
    pub fn new() -> Self {
        // This is somewhat cursed tbh, idk if there's a better way to embed the binary that doesn't complicate the build process
        // it also needs libc
        #[cfg(test)]
        return SkeletalInputIPC { connection: None };

        info!("Starting skeletal input...");
        let binary_path = get_library_path()
            .unwrap()
            .parent()
            .unwrap()
            .join("../../xrizer_skeletal_input");

        if !binary_path.exists() {
            error!("Skeletal input binary not found!")
        }

        // ipc-channels get transferred from the child to the parent over the server
        let (server, server_name) = IpcOneShotServer::new().unwrap();

        let child = Command::new(binary_path).env("IPC", server_name).spawn();

        match child {
            Ok(child) => {
                let (sender, receiver) = server.accept().unwrap().1;
                info!("Got IPC connection!");
                return SkeletalInputIPC {
                    connection: Some(IPCConnection {
                        child,
                        sender,
                        receiver,
                    }),
                };
            }
            Err(e) => {
                error!("Failed to spawn the skeletal input process: {:?}", e);
            }
        }

        SkeletalInputIPC { connection: None }
    }

    pub fn sync_actions(&self) -> Result<(), IpcError> {
        let Some(connection) = &self.connection else {
            return Err(IpcError::Disconnected);
        };
        match connection.sender.send(IPCMessage::SyncActions) {
            Ok(_) => match connection.receiver.recv()? {
                IPCMessage::Ack => Ok(()),
                msg => panic!("Unexpected IPC message: {:?}", msg),
            },
            Err(_) => Err(IpcError::Disconnected),
        }
    }

    pub fn get_action_states(&self, hand: Hand) -> Result<SkeletalInputActionStates, IpcError> {
        // We just tell the child which hand to grab, it does the sync actions call and sends back the data
        let Some(connection) = &self.connection else {
            return Err(IpcError::Disconnected);
        };
        match connection
            .sender
            .send(IPCMessage::GetHand(hand == Hand::Left))
        {
            Ok(_) => match connection.receiver.recv()? {
                IPCMessage::HandData(states) => Ok(states),
                msg => panic!("Unexpected IPC message: {:?}", msg),
            },
            Err(_) => Err(IpcError::Disconnected),
        }
    }
}

struct IPCConnection {
    child: Child,
    sender: IpcSender<IPCMessage>,
    receiver: IpcReceiver<IPCMessage>,
}

impl Drop for IPCConnection {
    fn drop(&mut self) {
        if let Err(e) = self.child.kill() {
            error!("Failed to kill child process: {}", e);
        }
        let _ = self.child.wait();
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum IPCMessage {
    SyncActions,
    GetHand(bool),
    HandData(SkeletalInputActionStates),
    Ack,
}

fn get_library_path() -> Option<PathBuf> {
    unsafe {
        let mut info: Dl_info = std::mem::zeroed();

        if dladdr(get_library_path as *const c_void, &mut info) != 0 {
            if let Ok(path) = CStr::from_ptr(info.dli_fname).to_str() {
                let path_string = path.to_string();
                return Some(Path::new(&path_string).to_owned());
            }
        }
    }
    None
}
