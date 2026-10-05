// Copyright 2021 Damir Jelić
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Modified for OMEMO 2 by the Chord authors, 2026.

pub(crate) mod message;
mod pre_key;

pub(crate) use message::Message;
pub(crate) use pre_key::PreKeyMessage;
use thiserror::Error;

/// An error from the parse of an OMEMO protobuf message.
#[derive(Debug, Error)]
pub enum DecodeError {
    /// The bytes are not a valid protobuf message.
    #[error("The message is not valid protobuf: {0}")]
    Proto(#[from] prost::DecodeError),
    /// The message is longer than the limit.
    #[error("The message is too long")]
    TooLong,
    /// A required field is missing.
    #[error("The field {0} is missing")]
    MissingField(&'static str),
    /// A public key is not in its canonical form.
    #[error("The key in the field {0} is not valid")]
    InvalidKey(&'static str),
    /// A field has the wrong length.
    #[error("The field {0} has the wrong length")]
    BadLength(&'static str),
}
