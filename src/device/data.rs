// Copyright (c) 2020-2021, Richard M Neswold, Jr.
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

use std::convert::TryInto;
use redis::*;

// `Type` defines the primitive types available to devices. Each
// enumeration value wraps a unique, native Rust type.

#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Nil,
    Bool(bool),
    Int(i64),
    Flt(f64),
    Str(String)
}

impl Type {
    #[doc(hidden)]
    fn decode_integer(buf: &[u8]) -> RedisResult<Self> {
	if buf.len() >= 8 {
	    let buf = buf[..8].try_into().unwrap();

	    return Ok(Type::Int(i64::from_be_bytes(buf)))
	}
	Err(RedisError::from((ErrorKind::TypeError, "integer data too short")))
    }

    #[doc(hidden)]
    fn decode_float(buf: &[u8]) -> RedisResult<Self> {
	if buf.len() >= 8 {
	    let buf = buf[..8].try_into().unwrap();

	    return Ok(Type::Flt(f64::from_be_bytes(buf)))
	}
	Err(RedisError::from((ErrorKind::TypeError,
			      "floating point data too short")))
    }

    #[doc(hidden)]
    fn decode_string(buf: &[u8]) -> RedisResult<Self> {
	if buf.len() >= 4 {
	    let len_buf = buf[..4].try_into().unwrap();
	    let len = u32::from_be_bytes(len_buf) as usize;

	    if buf.len() >= (4 + len) as usize {
		let str_vec = buf[4..4 + len].to_vec();

		return match String::from_utf8(str_vec) {
		    Ok(s) => Ok(Type::Str(s)),
		    Err(_) => Err(RedisError::from((ErrorKind::TypeError,
						    "string not UTF-8")))
		}
	    }
	}
	Err(RedisError::from((ErrorKind::TypeError, "string data too short")))
    }
}

// Implement the `ToRedisArgs` trait. This allows us to specify a
// `Type` when writing values to redis so they get encoded correctly.

impl ToRedisArgs for Type {
    fn write_redis_args<W>(&self, out: &mut W)
    where W: ?Sized + RedisWrite,
    {
	match self {
	    Type::Nil => out.write_arg(b""),
	    Type::Bool(false) => out.write_arg(b"F"),
	    Type::Bool(true) => out.write_arg(b"T"),

	    Type::Int(v) => {
		let mut buf: Vec<u8> = Vec::with_capacity(9);

		buf.push('I' as u8);
		buf.extend_from_slice(&v.to_be_bytes());
		out.write_arg(&buf)
	    },

	    Type::Flt(v) => {
		let mut buf: Vec<u8> = Vec::with_capacity(9);

		buf.push('D' as u8);
		buf.extend_from_slice(&v.to_be_bytes());
		out.write_arg(&buf)
	    },

	    Type::Str(s) => {
		let s = s.as_bytes();
		let mut buf: Vec<u8> = Vec::with_capacity(5 + s.len());

		buf.push('S' as u8);
		buf.extend_from_slice(&(s.len() as u32).to_be_bytes());
		buf.extend_from_slice(&s);
		out.write_arg(&buf)
	    }
	}
    }
}

// Implement the `FromRedisValue` trait. This trait tries to decode a
// `Type` from a string stored in redis.

impl FromRedisValue for Type {
    fn from_redis_value(v: &Value) -> RedisResult<Self>
    {
	if let Value::Data(buf) = v {

	    // The buffer has to have at least one character in order
	    // to be decoded.

	    if buf.len() > 0 {
		match buf[0] as char {
		    'F' => Ok(Type::Bool(false)),
		    'T' => Ok(Type::Bool(true)),
		    'I' => Self::decode_integer(&buf[1..]),
		    'D' => Self::decode_float(&buf[1..]),
		    'S' => Self::decode_string(&buf[1..]),

		    // Any other character in the tag field is unknown
		    // and can't be decoded as a `Type`.

		    _ =>
			Err(RedisError::from((ErrorKind::TypeError,
					      "unknown tag")))
		}
	    } else {
		Ok(Type::Nil)
	    }
	} else {
	    Err(RedisError::from((ErrorKind::TypeError, "bad redis::Value")))
	}
    }
}

pub trait Compat {
    fn to_type(self) -> Type;
}

impl Compat for bool {
    fn to_type(self) -> Type {
	Type::Bool(self)
    }
}

impl Compat for i64 {
    fn to_type(self) -> Type {
	Type::Int(self)
    }
}

impl Compat for f64 {
    fn to_type(self) -> Type {
	Type::Flt(self)
    }
}

impl Compat for String {
    fn to_type(self) -> Type {
	Type::Str(self)
    }
}

// This section holds code used for testing the module. The
// "#[cfg(test)]" attribute means the module will only be compiled and
// included in the test executable; debug and release versions won't
// have the code.

#[cfg(test)]
mod tests {
    use redis::{ from_redis_value, Value };
    use super::*;

    // We only want to convert Value::Data() forms. These tests make
    // sure the other variants don't translate.

    #[tokio::test]
    async fn test_reject_invalid_forms() {
	if let Ok(v) = from_redis_value::<Type>(&Value::Nil) {
	    panic!("Value::Nil incorrectly translated to {:?}", v);
	}
	if let Ok(v) = from_redis_value::<Type>(&Value::Int(0)) {
	    panic!("Value::Int incorrectly translated to {:?}", v);
	}
	if let Ok(v) = from_redis_value::<Type>(&Value::Bulk(vec![])) {
	    panic!("Value::Bulk incorrectly translated to {:?}", v);
	}
	if let Ok(v) = from_redis_value::<Type>(&Value::Status(String::from(""))) {
	    panic!("Value::Status incorrectly translated to {:?}", v);
	}
	if let Ok(v) = from_redis_value::<Type>(&Value::Okay) {
	    panic!("Value::Okay incorrectly translated to {:?}", v);
	}
    }

    // Test correct Type::Nil decoding.

    #[tokio::test]
    async fn test_nil_decoder() {
	assert_eq!(Ok(Type::Nil),
		   from_redis_value::<Type>(&Value::Data(vec![])));
    }

    // Test correct Type::Bool decoding.

    #[tokio::test]
    async fn test_bool_decoder() {
	assert_eq!(Ok(Type::Bool(false)),
		   from_redis_value::<Type>(&Value::Data(vec!['F' as u8])));
	assert_eq!(Ok(Type::Bool(true)),
		   from_redis_value::<Type>(&Value::Data(vec!['T' as u8])));
    }

    // Test correct Type::Int decoding.

    #[tokio::test]
    async fn test_int_decoder() {
	let values: Vec<(i64, Vec<u8>)> = vec![
	    (0, vec!['I' as u8,
		     0x00u8, 0x00u8, 0x00u8, 0x00u8,
		     0x00u8, 0x00u8, 0x00u8, 0x00u8]),
	    (1, vec!['I' as u8,
		     0x00u8, 0x00u8, 0x00u8, 0x00u8,
		     0x00u8, 0x00u8, 0x00u8, 0x01u8]),
	    (-1, vec!['I' as u8,
		      0xffu8, 0xffu8, 0xffu8, 0xffu8,
		      0xffu8, 0xffu8, 0xffu8, 0xffu8]),
	    (0x7fffffffffffffff, vec!['I' as u8,
				      0x7fu8, 0xffu8, 0xffu8, 0xffu8,
				      0xffu8, 0xffu8, 0xffu8, 0xffu8]),
	    (-0x8000000000000000, vec!['I' as u8,
				       0x80u8, 0x00u8, 0x00u8, 0x00u8,
				       0x00u8, 0x00u8, 0x00u8, 0x00u8]),
	    (0x0123456789abcdef, vec!['I' as u8,
				      0x01u8, 0x23u8, 0x45u8, 0x67u8,
				      0x89u8, 0xabu8, 0xcdu8, 0xefu8]),
	];

	for (v, rv) in values.iter() {
	    let data = Value::Data(rv.to_vec());

	    assert_eq!(Ok(Type::Int(*v)), from_redis_value(&data));
	}
    }

    #[tokio::test]
    async fn test_nil_encoder() {
	assert_eq!(vec![ Vec::<u8>::new() ], Type::Nil.to_redis_args());
    }

    #[tokio::test]
    async fn test_bool_encoder() {
	assert_eq!(vec![ vec![ 'F' as u8] ],
		   Type::Bool(false).to_redis_args());
	assert_eq!(vec![ vec![ 'T' as u8] ],
		   Type::Bool(true).to_redis_args());
    }

    #[tokio::test]
    async fn test_int_encoder() {
	let values: Vec<(i64, Vec<Vec<u8>>)> = vec![
	    (0, vec![ vec![ 'I' as u8, 0x00, 0x00, 0x00, 0x00,
			     0x00, 0x00, 0x00, 0x00 ] ]),
	    (1, vec![ vec![ 'I' as u8, 0x00, 0x00, 0x00, 0x00,
			     0x00, 0x00, 0x00, 0x01 ] ]),
	    (-1, vec![ vec![ 'I' as u8, 0xff, 0xff, 0xff, 0xff,
			      0xff, 0xff, 0xff, 0xff ] ]),
	    (0x7fffffffffffffff,
	     vec![ vec![ 'I' as u8, 0x7f, 0xff, 0xff, 0xff,
			  0xff, 0xff, 0xff, 0xff ] ]),
	    (-0x8000000000000000,
	     vec![ vec![ 'I' as u8, 0x80, 0x00, 0x00, 0x00,
			  0x00, 0x00, 0x00, 0x00 ] ]),
	    (0x0123456789abcdef,
	     vec![ vec![ 'I' as u8, 0x01, 0x23, 0x45, 0x67,
			  0x89, 0xab, 0xcd, 0xef ] ]),
	];

	for (v, rv) in values.iter() {
	    assert_eq!(*rv, Type::Int(*v).to_redis_args());
	}
    }
}
