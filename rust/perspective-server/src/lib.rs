// ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
// ┃ ██████ ██████ ██████       █      █      █      █      █ █▄  ▀███ █       ┃
// ┃ ▄▄▄▄▄█ █▄▄▄▄▄ ▄▄▄▄▄█  ▀▀▀▀▀█▀▀▀▀▀ █ ▀▀▀▀▀█ ████████▌▐███ ███▄  ▀█ █ ▀▀▀▀▀ ┃
// ┃ █▀▀▀▀▀ █▀▀▀▀▀ █▀██▀▀ ▄▄▄▄▄ █ ▄▄▄▄▄█ ▄▄▄▄▄█ ████████▌▐███ █████▄   █ ▄▄▄▄▄ ┃
// ┃ █      ██████ █  ▀█▄       █ ██████      █      ███▌▐███ ███████▄ █       ┃
// ┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
// ┃ Copyright (c) 2017, the Perspective Authors.                              ┃
// ┃ ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌ ┃
// ┃ This file is part of the Perspective library, distributed under the terms ┃
// ┃ of the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0). ┃
// ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

//! This crate contains the server/engine components of the
//! [Perspective](https://perspective.finos.org) data visualization suite. It is
//! meant to be used in conjunction with the other crates of this project,
//! e.g. `perspective-client` to create client connections to a server.
//!
//! The [`perspective`] crate provides a convenient frontend for Rust
//! developers, including both [`perspective_client`] and [`perspective_server`]
//! as well as other convenient integration helpers.
//!
//! # Architecture
//!
//! The basic dataflow of a Perspective applications looks something like this:
//!                                                                             
//! ```text
//!                      : Network or sync boundary
//!                      :
//!  Client 1            :   Session 1                      Server
//! ┏━━━━━━━━━━━━━━━━━━┓ :  ┏━━━━━━━━━━━━━━━━━━┓           ┏━━━━━━━━━━━━━━━━━━┓
//! ┃ handle_response  ┃<━━━┃ send_response    ┃<━┳━━━━━━━━┃ send_response    ┃
//! ┃ send_request     ┃━┳━>┃ handle_request   ┃━━━━━┳━━━━>┃ handle_request   ┃
//! ┗━━━━━━━━━━━━━━━━━━┛ ┗━>┃ poll             ┃━━━━━━━━┳━>┃ poll             ┃
//!                      :  ┃ session_id       ┃  ┃  ┃  ┃  ┃ generate_id      ┃
//!                      :  ┗━━━━━━━━━━━━━━━━━━┛  ┃  ┃  ┃  ┃ cleanup_id       ┃
//!                      :                        ┃  ┃  ┃  ┗━━━━━━━━━━━━━━━━━━┛
//!  Client 2            :   Session 2            ┃  ┃  ┃
//! ┏━━━━━━━━━━━━━━━━━━┓ :  ┏━━━━━━━━━━━━━━━━━━┓  ┃  ┃  ┃  
//! ┃ handle_response  ┃<━━━┃ send_response    ┃<━┛  ┃  ┃                         
//! ┃ send_request     ┃━┳━>┃ handle_request   ┃━━━━━┛  ┃                                        
//! ┗━━━━━━━━━━━━━━━━━━┛ ┗━>┃ poll             ┃━━━━━━━━┛
//!                      :  ┃ session_id       ┃                                                 
//!                      :  ┗━━━━━━━━━━━━━━━━━━┛
//! ```
//!
//! # Feature Flags
//!
//! The following feature flags are available to enable in your `Cargo.toml`:
//!
//! - `external-cpp` Set this flag to configure this crate's compile process to
//!   look for Perspective C++ source code in the environment rather than
//!   locally, e.g. for when you build this crate in-place in the Perspective
//!   repo source tree.

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use async_lock::RwLock;
use cxx::UniquePtr;
use futures::future::BoxFuture;
use futures::Future;

mod ffi;

pub type ServerError = Box<dyn Error + Send + Sync>;

type SessionCallback =
    Arc<dyn for<'a> Fn(&'a [u8]) -> BoxFuture<'a, Result<(), ServerError>> + Send + Sync>;

/// Use [`SessionHandler`] to implement a callback for messages emitted from
/// a [`Session`], to be passed to the [`Server::new_session`] constructor.
/// Alternatively, a [`Session`] can be created from a closure instead via
/// [`Server::new_session_with_callback`].
///                                                                         
/// ```text
///                      :
///  Client              :   Session
/// ┏━━━━━━━━━━━━━━━━━━┓ :  ┏━━━━━━━━━━━━━━━━━━━┓
/// ┃ handle_response  ┃<━━━┃ send_response (*) ┃
/// ┃ ..               ┃ :  ┃ ..                ┃
/// ┗━━━━━━━━━━━━━━━━━━┛ :  ┗━━━━━━━━━━━━━━━━━━━┛
///                      :
/// ```
pub trait SessionHandler: Send + Sync {
    /// Dispatch a message from a [`Server`] for a the [`Session`] that took
    /// this `SessionHandler` instance as a constructor argument.
    fn send_response<'a>(
        &'a mut self,
        msg: &'a [u8],
    ) -> impl Future<Output = Result<(), ServerError>> + Send + 'a;
}

/// An instance of a Perspective server. Each [`Server`] instance is separate,
/// and does not share [`perspective_client::Table`] (or other) data with other
/// [`Server`]s.
#[derive(Clone)]
pub struct Server {
    server: Arc<UniquePtr<ffi::ProtoApiServer>>,
    callbacks: Arc<RwLock<HashMap<u32, SessionCallback>>>,
}

impl Default for Server {
    fn default() -> Self {
        let server = Arc::new(ffi::new_proto_server());
        let callbacks = Arc::default();
        Self { server, callbacks }
    }
}

impl Server {
    /// An alternative method for creating a new [`Session`] for this
    /// [`Server`], from a callback closure instead of a via a trait.
    /// See [`Server::new_session`] for details.
    ///
    /// # Arguments
    ///
    /// - `send_response` -  A function invoked by the [`Server`] when a
    ///   response message needs to be sent to the
    ///   [`perspective_client::Client`].
    pub async fn new_session_with_callback<F>(&self, send_response: F) -> Session
    where
        F: for<'a> Fn(&'a [u8]) -> BoxFuture<'a, Result<(), ServerError>> + 'static + Sync + Send,
    {
        let id = ffi::new_session(&self.server);
        let server = self.clone();
        self.callbacks
            .write()
            .await
            .insert(id, Arc::new(send_response));

        Session {
            id,
            server,
            closed: false,
        }
    }

    /// Create a [`Session`] for this [`Server`], suitable for exactly one
    /// [`perspective_client::Client`] (not necessarily in this process). A
    /// [`Session`] represents the server-side state of a single
    /// client-to-server connection.
    ///
    /// # Arguments
    ///
    /// - `session_handler` - An implementor of [`SessionHandler`] which will be
    ///   invoked by the [`Server`] when a response message needs to be sent to
    ///   the [`Client`]. The response itself should be passed to
    ///   [`Client::handle_response`] eventually, though it may-or-may-not be in
    ///   the same process.
    pub async fn new_session<F>(&self, session_handler: F) -> Session
    where
        F: SessionHandler + 'static + Sync + Send + Clone,
    {
        self.new_session_with_callback(move |msg| {
            let mut session_handler = session_handler.clone();
            Box::pin(async move { session_handler.send_response(msg).await })
        })
        .await
    }

    async fn handle_request(&self, client_id: u32, val: &[u8]) -> Result<(), ServerError> {
        for response in ffi::handle_request(&self.server, client_id, val).0 {
            let cb = self
                .callbacks
                .read()
                .await
                .get(&response.client_id)
                .cloned();

            if let Some(f) = cb {
                f(&response.resp).await?
            }
        }

        Ok(())
    }

    async fn poll(&self) -> Result<(), ServerError> {
        for response in ffi::poll(&self.server).0 {
            let cb = self
                .callbacks
                .read()
                .await
                .get(&response.client_id)
                .cloned();

            if let Some(f) = cb {
                f(&response.resp).await?
            }
        }

        Ok(())
    }

    async fn close(&self, client_id: u32) {
        ffi::close_session(&self.server, client_id);
        self.callbacks
            .write()
            .await
            .remove(&client_id)
            .expect("Already closed");
    }
}

/// The server-side representation of a connection to a
/// [`perspective_client::Client`]. For each [`perspective_client::Client`] that
/// wants to connect to a [`Server`], a dedicated [`Session`] must be created.
/// The [`Session`] handles routing messages emitted by the [`Server`], as well
/// as owning any resources the [`Client`] may request.
pub struct Session {
    id: u32,
    server: Server,
    closed: bool,
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.closed {
            tracing::error!("`Session` dropped without `Session::close`");
        }
    }
}

impl Session {
    /// Handle an incoming request from the [`Client`]. Calling
    /// [`Session::handle_request`] will result in the `send_response` parameter
    /// which was used to construct this [`Session`] to fire one or more times.
    ///
    /// ```text
    ///                      :
    ///  Client              :   Session
    /// ┏━━━━━━━━━━━━━━━━━━┓ :  ┏━━━━━━━━━━━━━━━━━━━━┓
    /// ┃ send_request     ┃━━━>┃ handle_request (*) ┃
    /// ┃ ..               ┃ :  ┃ ..                 ┃
    /// ┗━━━━━━━━━━━━━━━━━━┛ :  ┗━━━━━━━━━━━━━━━━━━━━┛
    ///                      :
    /// ```
    ///
    /// # Arguments
    ///
    /// - `request` An incoming request message, generated from a
    ///   [`Client::new`]'s `send_request` handler (which may-or-may-not be
    ///   local).
    pub async fn handle_request(&self, request: &[u8]) -> Result<(), ServerError> {
        self.server.handle_request(self.id, request).await
    }

    /// Flush any pending messages which may have resulted from previous
    /// [`Session::handle_request`] calls. Calling [`Session::poll`] may result
    /// in the `send_response` parameter which was used to construct this (or
    /// other) [`Session`] to fire. Whenever a [`Session::handle_request`]
    /// method is invoked for a [`Server`], at least one [`Session::poll`]
    /// should be scheduled to clear other clients message queues.
    ///
    /// ```text
    ///                      :
    ///  Client              :   Session                  Server
    /// ┏━━━━━━━━━━━━━━━━━━┓ :  ┏━━━━━━━━━━━━━━━━━━━┓
    /// ┃ send_request     ┃━┳━>┃ handle_request    ┃    ┏━━━━━━━━━━━━━━━━━━━┓
    /// ┃ ..               ┃ ┗━>┃ poll (*)          ┃━━━>┃ poll (*)          ┃
    /// ┗━━━━━━━━━━━━━━━━━━┛ :  ┃ ..                ┃    ┃ ..                ┃
    ///                      :  ┗━━━━━━━━━━━━━━━━━━━┛    ┗━━━━━━━━━━━━━━━━━━━┛
    /// ```
    pub async fn poll(&self) -> Result<(), ServerError> {
        self.server.poll().await
    }

    /// Close this [`Session`], cleaning up any callbacks (e.g. arguments
    /// provided to [`Session::handle_request`] or
    /// [`perspective_client::View::OnUpdate`]) and resources (e.g. views
    /// returned by a call to [`perspective_client::Table::view`]).
    /// Dropping a [`Session`] outside of the context of [`Session::close`]
    /// will cause a [`tracing`] error-level log to be emitted, but won't fail.
    /// They will, however, leak.
    pub async fn close(mut self) {
        self.closed = true;
        self.server.close(self.id).await
    }
}
