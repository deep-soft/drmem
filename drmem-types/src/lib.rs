// Copyright (c) 2020-2022, Richard M Neswold, Jr.
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its
//    contributors may be used to endorse or promote products derived
//    from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! Defines fundamental types used throughout the DrMem codebase.

use std::fmt;

/// Enumerates all the errors that can be reported in DrMem. Authors
/// for new drivers or storage backends should try to map their
/// errors into one of these values. If no current value is
/// appropriate, a new one could be added (requiring a new release of
/// this crate) but make sure the new error code is generic enough
/// that it may be useful for other drivers or backends. For instance,
/// don't add an error value that is specific to Redis. Add a more
/// general value and use the associated description string to explain
/// the details.

#[derive(Debug, PartialEq)]
pub enum Error {
    /// Returned whenever a resource cannot be found.
    NotFound,

    /// A resource is already in use.
    InUse,

    /// The device name is already registered to another driver.
    DeviceDefined(String),

    /// Reported when the peer of a communication channel has closed
    /// its handle.
    MissingPeer(String),

    /// A type mismatch is preventing the operation from continuing.
    TypeError,

    /// An invalid value was provided.
    InvArgument(&'static str),

    /// Returned when a communication error occurred with the backend
    /// database. Each backend will have its own recommendations on
    /// how to recover.
    DbCommunicationError,

    /// The requested operation cannot complete because the process
    /// hasn't provided proper authentication credentials.
    AuthenticationError,

    /// The requested operation couldn't complete. The description
    /// field will have more information for the user.
    OperationError,

    /// A bad parameter was given in a configuration or a
    /// configuration was missing a required parameter.
    BadConfig,

    /// A dependent library introduced a new error that hasn't been
    /// properly mapped in DrMem. This needs to be reported as a bug.
    UnknownError,
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::NotFound => write!(f, "item not found"),
            Error::InUse => write!(f, "item is in use"),
            Error::DeviceDefined(name) => {
                write!(f, "device {} is already defined", &name)
            }
            Error::MissingPeer(detail) => {
                write!(f, "{} is missing peer", detail)
            }
            Error::TypeError => write!(f, "incorrect type"),
            Error::InvArgument(s) => write!(f, "{}", s),
            Error::DbCommunicationError => {
                write!(f, "db communication error")
            }
            Error::AuthenticationError => write!(f, "permission error"),
            Error::OperationError => {
                write!(f, "couldn't complete operation")
            }
            Error::BadConfig => write!(f, "bad configuration"),
            Error::UnknownError => write!(f, "unhandled error"),
        }
    }
}

pub mod device;
