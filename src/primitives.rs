// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0
//! Serialization support for primitive data types.
#![allow(clippy::float_cmp)]

use self::super::{VersionMap, Versionize, VersionizeError, VersionizeResult};
use vmm_sys_util::fam::{FamStruct, FamStructWrapper};

/// Maximum allowed string len in bytes (16KB).
/// Calling `serialize()` or `deserialiaze()` will fail beyond this limit.
pub const MAX_STRING_LEN: usize = 16384;
/// Maximum allowed vec size in bytes (10MB).
/// Calling `serialize()` or `deserialiaze()` will fail beyond this limit.
pub const MAX_VEC_SIZE: usize = 10_485_760;

/// A macro that implements the Versionize trait for primitive types using the
/// serde bincode backed.
macro_rules! impl_versionize {
    ($ty:ident) => {
        impl Versionize for $ty {
            #[inline]
            fn serialize<W: std::io::Write>(
                &self,
                writer: &mut W,
                _version_map: &VersionMap,
                _version: u16,
            ) -> VersionizeResult<()> {
                bincode::serialize_into(writer, &self)
                    .map_err(|ref err| VersionizeError::Serialize(format!("{:?}", err)))?;
                Ok(())
            }

            #[inline]
            fn deserialize<R: std::io::Read>(
                mut reader: &mut R,
                _version_map: &VersionMap,
                _version: u16,
            ) -> VersionizeResult<Self>
            where
                Self: Sized,
            {
                Ok(bincode::deserialize_from(&mut reader)
                    .map_err(|ref err| VersionizeError::Deserialize(format!("{:?}", err)))?)
            }

            // Not used.
            fn version() -> u16 {
                1
            }
        }
    };
}

impl_versionize!(bool);
impl_versionize!(isize);
impl_versionize!(i8);
impl_versionize!(i16);
impl_versionize!(i32);
impl_versionize!(i64);
impl_versionize!(usize);
impl_versionize!(u8);
impl_versionize!(u16);
impl_versionize!(u32);
impl_versionize!(u64);
impl_versionize!(f32);
impl_versionize!(f64);
impl_versionize!(char);

impl Versionize for String {
    #[inline]
    fn serialize<W: std::io::Write>(
        &self,
        writer: &mut W,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<()> {
        // It is better to fail early at serialization time.
        if self.len() > MAX_STRING_LEN {
            return Err(VersionizeError::StringLength(self.len()));
        }

        self.len().serialize(writer, version_map, app_version)?;
        writer
            .write_all(self.as_bytes())
            .map_err(|e| VersionizeError::Io(e.raw_os_error().unwrap_or(0)))?;
        Ok(())
    }

    #[inline]
    fn deserialize<R: std::io::Read>(
        mut reader: &mut R,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<Self> {
        let len = usize::deserialize(&mut reader, version_map, app_version)?;
        // Even if we fail in serialize, we still need to enforce this on the hot path
        // in case the len is corrupted.
        if len > MAX_STRING_LEN {
            return Err(VersionizeError::StringLength(len));
        }

        let mut v = vec![0u8; len];
        reader
            .read_exact(v.as_mut_slice())
            .map_err(|e| VersionizeError::Io(e.raw_os_error().unwrap_or(0)))?;
        Ok(String::from_utf8(v)
            .map_err(|err| VersionizeError::Deserialize(format!("Utf8 error: {:?}", err)))?)
    }

    // Not used yet.
    fn version() -> u16 {
        1
    }
}

macro_rules! impl_versionize_array_with_size {
    ($ty:literal) => {
        impl<T> Versionize for [T; $ty]
        where
            T: Copy + Default + Versionize,
        {
            #[inline]
            fn serialize<W: std::io::Write>(
                &self,
                writer: &mut W,
                version_map: &VersionMap,
                app_version: u16,
            ) -> VersionizeResult<()> {
                for element in self {
                    element.serialize(writer, version_map, app_version)?;
                }

                Ok(())
            }

            #[inline]
            fn deserialize<R: std::io::Read>(
                reader: &mut R,
                version_map: &VersionMap,
                app_version: u16,
            ) -> VersionizeResult<Self> {
                let mut array = [T::default(); $ty];
                for i in 0..$ty {
                    array[i] = T::deserialize(reader, version_map, app_version)?;
                }
                Ok(array)
            }

            // Not used yet.
            fn version() -> u16 {
                1
            }
        }
    };
}

// Conventionally, traits are available for primitive arrays only up to size 32
// until the const generics feature is implemented.
// [https://doc.rust-lang.org/std/primitive.array.html]
// [https://github.com/rust-lang/rust/issues/44580]
macro_rules! impl_versionize_arrays {
    ($($N:literal)+) => {
        $(
            impl_versionize_array_with_size!($N);
        )+
    }
}

#[allow(clippy::reversed_empty_ranges)]
impl_versionize_arrays! {
    1  2  3  4  5  6  7  8  9 10
   11 12 13 14 15 16 17 18 19 20
   21 22 23 24 25 26 27 28 29 30
   31 32
}

impl<T> Versionize for Box<T>
where
    T: Versionize,
{
    #[inline]
    fn serialize<W: std::io::Write>(
        &self,
        writer: &mut W,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<()> {
        self.as_ref().serialize(writer, version_map, app_version)
    }

    #[inline]
    fn deserialize<R: std::io::Read>(
        reader: &mut R,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<Self> {
        Ok(Box::new(T::deserialize(reader, version_map, app_version)?))
    }

    // Not used yet.
    fn version() -> u16 {
        1
    }
}

impl<T> Versionize for std::num::Wrapping<T>
where
    T: Versionize,
{
    #[inline]
    fn serialize<W: std::io::Write>(
        &self,
        writer: &mut W,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<()> {
        self.0.serialize(writer, version_map, app_version)
    }

    #[inline]
    fn deserialize<R: std::io::Read>(
        reader: &mut R,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<Self> {
        Ok(std::num::Wrapping(T::deserialize(
            reader,
            version_map,
            app_version,
        )?))
    }

    // Not used yet.
    fn version() -> u16 {
        1
    }
}

impl<T> Versionize for Option<T>
where
    T: Versionize,
{
    #[inline]
    fn serialize<W: std::io::Write>(
        &self,
        writer: &mut W,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<()> {
        // Serialize an Option just like bincode does: u8, T.
        match &*self {
            Some(value) => {
                1u8.serialize(writer, version_map, app_version)?;
                value.serialize(writer, version_map, app_version)
            }
            None => 0u8.serialize(writer, version_map, app_version),
        }
    }

    #[inline]
    fn deserialize<R: std::io::Read>(
        reader: &mut R,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<Self> {
        let option = u8::deserialize(reader, version_map, app_version)?;
        match option {
            0u8 => Ok(None),
            1u8 => Ok(Some(T::deserialize(reader, version_map, app_version)?)),
            value => Err(VersionizeError::Deserialize(format!(
                "Invalid option value {}",
                value
            ))),
        }
    }

    // Not used yet.
    fn version() -> u16 {
        1
    }
}

impl<T> Versionize for Vec<T>
where
    T: Versionize,
{
    #[inline]
    fn serialize<W: std::io::Write>(
        &self,
        mut writer: &mut W,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<()> {
        if self.len() > MAX_VEC_SIZE / std::mem::size_of::<T>() {
            return Err(VersionizeError::VecLength(self.len()));
        }
        // Serialize in the same fashion as bincode:
        // Write len.
        bincode::serialize_into(&mut writer, &self.len())
            .map_err(|ref err| VersionizeError::Serialize(format!("{:?}", err)))?;
        // Walk the vec and write each element.
        for element in self {
            element.serialize(writer, version_map, app_version)?;
        }
        Ok(())
    }

    #[inline]
    fn deserialize<R: std::io::Read>(
        mut reader: &mut R,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<Self> {
        let mut v = Vec::new();
        let len: usize = bincode::deserialize_from(&mut reader)
            .map_err(|ref err| VersionizeError::Deserialize(format!("{:?}", err)))?;

        if len > MAX_VEC_SIZE / std::mem::size_of::<T>() {
            return Err(VersionizeError::VecLength(len));
        }

        for _ in 0..len {
            let element: T = T::deserialize(reader, version_map, app_version)
                .map_err(|ref err| VersionizeError::Deserialize(format!("{:?}", err)))?;
            v.push(element);
        }
        Ok(v)
    }

    // Not used yet.
    fn version() -> u16 {
        1
    }
}

// Implement versioning for FAM structures by using the FamStructWrapper interface.
impl<T: Default + FamStruct + Versionize> Versionize for FamStructWrapper<T>
where
    <T as FamStruct>::Entry: Versionize,
    T: std::fmt::Debug,
{
    #[inline]
    fn serialize<W: std::io::Write>(
        &self,
        mut writer: &mut W,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<()> {
        // Write the fixed size header.
        self.as_fam_struct_ref()
            .serialize(&mut writer, version_map, app_version)?;
        // Write the array.
        self.as_slice()
            .to_vec()
            .serialize(&mut writer, version_map, app_version)?;

        Ok(())
    }

    #[inline]
    fn deserialize<R: std::io::Read>(
        reader: &mut R,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<Self> {
        let header = T::deserialize(reader, version_map, app_version)
            .map_err(|ref err| VersionizeError::Deserialize(format!("{:?}", err)))?;
        let entries: Vec<<T as FamStruct>::Entry> =
            Vec::deserialize(reader, version_map, app_version)
                .map_err(|ref err| VersionizeError::Deserialize(format!("{:?}", err)))?;
        // Construct the object from the array items.
        // Header(T) fields will be initialized by Default trait impl.
        let mut object = FamStructWrapper::from_entries(&entries)
            .map_err(|ref err| VersionizeError::Deserialize(format!("{:?}", err)))?;
        // Update Default T with the deserialized header.
        *object.as_mut_fam_struct() = header;
        Ok(object)
    }

    // Not used.
    fn version() -> u16 {
        1
    }
}

// Manual implementation for tuple of 2 elems.
impl<T: Versionize, U: Versionize> Versionize for (T, U) {
    #[inline]
    fn serialize<W: std::io::Write>(
        &self,
        writer: &mut W,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<()> {
        self.0.serialize(writer, version_map, app_version)?;
        self.1.serialize(writer, version_map, app_version)?;
        Ok(())
    }

    #[inline]
    fn deserialize<R: std::io::Read>(
        reader: &mut R,
        version_map: &VersionMap,
        app_version: u16,
    ) -> VersionizeResult<Self> {
        Ok((
            T::deserialize(reader, version_map, app_version)?,
            U::deserialize(reader, version_map, app_version)?,
        ))
    }

    // Not used yet.
    fn version() -> u16 {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{VersionMap, Versionize, VersionizeResult};

    // Generate primitive tests using this macro.
    macro_rules! primitive_int_test {
        ($ty:ident, $fn_name:ident) => {
            #[test]
            fn $fn_name() {
                let vm = VersionMap::new();
                let mut snapshot_mem = vec![0u8; 64];

                let store: $ty = std::$ty::MAX;
                store
                    .serialize(&mut snapshot_mem.as_mut_slice(), &vm, 1)
                    .unwrap();
                let restore =
                    <$ty as Versionize>::deserialize(&mut snapshot_mem.as_slice(), &vm, 1).unwrap();

                assert_eq!(store, restore);
            }
        };
    }

    primitive_int_test!(usize, test_ser_de_usize);
    primitive_int_test!(isize, test_ser_de_isize);
    primitive_int_test!(u8, test_ser_de_u8);
    primitive_int_test!(u16, test_ser_de_u16);
    primitive_int_test!(u32, test_ser_de_u32);
    primitive_int_test!(u64, test_ser_de_u64);
    primitive_int_test!(i8, test_ser_de_i8);
    primitive_int_test!(i16, test_ser_de_i16);
    primitive_int_test!(i32, test_ser_de_i32);
    primitive_int_test!(i64, test_ser_de_i64);
    primitive_int_test!(f32, test_ser_de_f32);
    primitive_int_test!(f64, test_ser_de_f64);
    primitive_int_test!(char, test_ser_de_char);

    #[test]
    fn test_corrupted_string_len() {
        let vm = VersionMap::new();
        let mut buffer = vec![0u8; 1024];

        let string = String::from("Test string1");
        string
            .serialize(&mut buffer.as_mut_slice(), &vm, 1)
            .unwrap();

        // Test corrupt length field.
        assert_eq!(
            <String as Versionize>::deserialize(
                &mut buffer.as_slice().split_first().unwrap().1,
                &vm,
                1
            )
            .unwrap_err(),
            VersionizeError::StringLength(6052837899185946624)
        );

        // Test incomplete string.
        assert_eq!(
            <String as Versionize>::deserialize(&mut buffer.as_slice().split_at(6).0, &vm, 1)
                .unwrap_err(),
            VersionizeError::Deserialize(
                "Io(Custom { kind: UnexpectedEof, error: \"failed to fill whole buffer\" })"
                    .to_owned()
            )
        );

        // Test NULL string len.
        buffer[0] = 0;
        assert_eq!(
            <String as Versionize>::deserialize(&mut buffer.as_slice(), &vm, 1).unwrap(),
            String::new()
        );
    }

    #[test]
    fn test_corrupted_vec_len() {
        let vm = VersionMap::new();
        let mut buffer = vec![0u8; 1024];

        let mut string = String::from("Test string1");
        let vec = unsafe { string.as_mut_vec() };
        vec.serialize(&mut buffer.as_mut_slice(), &vm, 1).unwrap();

        // Test corrupt length field.
        assert_eq!(
            <Vec<u8> as Versionize>::deserialize(
                &mut buffer.as_slice().split_first().unwrap().1,
                &vm,
                1
            )
            .unwrap_err(),
            VersionizeError::VecLength(6052837899185946624)
        );

        // Test incomplete Vec.
        assert_eq!(
            <Vec<u8> as Versionize>::deserialize(&mut buffer.as_slice().split_at(6).0, &vm, 1)
                .unwrap_err(),
            VersionizeError::Deserialize(
                "Io(Custom { kind: UnexpectedEof, error: \"failed to fill whole buffer\" })"
                    .to_owned()
            )
        );

        // Test NULL Vec len.
        buffer[0] = 0;
        assert_eq!(
            <Vec<u8> as Versionize>::deserialize(&mut buffer.as_slice(), &vm, 1).unwrap(),
            Vec::new()
        );
    }

    #[test]
    fn test_ser_de_u32_tuple() {
        let vm = VersionMap::new();
        let mut snapshot_mem = vec![0u8; 64];

        let store: (u32, u32) = (std::u32::MIN, std::u32::MAX);
        store
            .serialize(&mut snapshot_mem.as_mut_slice(), &vm, 1)
            .unwrap();
        let restore =
            <(u32, u32) as Versionize>::deserialize(&mut snapshot_mem.as_slice(), &vm, 1).unwrap();

        assert_eq!(store, restore);
    }

    #[derive(Debug, serde_derive::Deserialize, PartialEq, serde_derive::Serialize, Versionize)]
    enum CompatibleEnum {
        A,
        B(String),
        C(u64, u64, char),
    }

    #[derive(Debug, serde_derive::Deserialize, PartialEq, serde_derive::Serialize, Versionize)]
    struct TestCompatibility {
        _string: String,
        _array: [u8; 32],
        _u8: u8,
        _u16: u16,
        _u32: u32,
        _u64: u64,
        _i8: i8,
        _i16: i16,
        _i32: i32,
        _i64: i64,
        _f32: f32,
        _f64: f64,
        _usize: usize,
        _isize: isize,
        _vec: Vec<u64>,
        _option: Option<bool>,
        _enums: Vec<CompatibleEnum>,
        _box: Box<String>,
    }

    #[test]
    fn test_bincode_deserialize_from_versionize() {
        let mut snapshot_mem = vec![0u8; 4096];
        let vm = VersionMap::new();

        let test_struct = TestCompatibility {
            _string: "String".to_owned(),
            _array: [128u8; 32],
            _u8: 1,
            _u16: 32000,
            _u32: 0x1234_5678,
            _u64: 0x1234_5678_9875_4321,
            _i8: -1,
            _i16: -32000,
            _i32: -0x1234_5678,
            _i64: -0x1234_5678_9875_4321,
            _usize: 0x1234_5678_9875_4321,
            _isize: -0x1234_5678_9875_4321,
            _f32: 0.123,
            _f64: 0.123_456_789_000_000,
            _vec: vec![33; 32],
            _option: Some(true),
            _enums: vec![
                CompatibleEnum::A,
                CompatibleEnum::B("abcd".to_owned()),
                CompatibleEnum::C(1, 2, 'a'),
            ],
            _box: Box::new("Box".to_owned()),
        };

        Versionize::serialize(&test_struct, &mut snapshot_mem.as_mut_slice(), &vm, 1).unwrap();

        let restored_state: TestCompatibility =
            bincode::deserialize_from(snapshot_mem.as_slice()).unwrap();
        assert_eq!(test_struct, restored_state);
    }

    #[test]
    fn test_bincode_serialize_to_versionize() {
        let mut snapshot_mem = vec![0u8; 4096];
        let vm = VersionMap::new();

        let test_struct = TestCompatibility {
            _string: "String".to_owned(),
            _array: [128u8; 32],
            _u8: 1,
            _u16: 32000,
            _u32: 0x1234_5678,
            _u64: 0x1234_5678_9875_4321,
            _i8: -1,
            _i16: -32000,
            _i32: -0x1234_5678,
            _i64: -0x1234_5678_9875_4321,
            _usize: 0x1234_5678_9875_4321,
            _isize: -0x1234_5678_9875_4321,
            _f32: 0.123,
            _f64: 0.123_456_789_000_000,
            _vec: vec![33; 32],
            _option: Some(true),
            _enums: vec![
                CompatibleEnum::A,
                CompatibleEnum::B("abcd".to_owned()),
                CompatibleEnum::C(1, 2, 'a'),
            ],
            _box: Box::new("Box".to_owned()),
        };

        bincode::serialize_into(&mut snapshot_mem.as_mut_slice(), &test_struct).unwrap();

        let restored_state: TestCompatibility =
            Versionize::deserialize(&mut snapshot_mem.as_slice(), &vm, 1).unwrap();
        assert_eq!(test_struct, restored_state);
    }

    #[test]
    fn test_ser_de_bool() {
        let vm = VersionMap::new();
        let mut snapshot_mem = vec![0u8; 64];

        let store = true;
        store
            .serialize(&mut snapshot_mem.as_mut_slice(), &vm, 1)
            .unwrap();
        let restore =
            <bool as Versionize>::deserialize(&mut snapshot_mem.as_slice(), &vm, 1).unwrap();

        assert_eq!(store, restore);
    }

    #[test]
    fn test_ser_de_string() {
        let vm = VersionMap::new();
        let mut snapshot_mem = vec![0u8; 64];

        let store = String::from("test string");
        store
            .serialize(&mut snapshot_mem.as_mut_slice(), &vm, 1)
            .unwrap();
        let restore =
            <String as Versionize>::deserialize(&mut snapshot_mem.as_slice(), &vm, 1).unwrap();

        assert_eq!(store, restore);
    }

    #[test]
    fn test_ser_de_vec() {
        let vm = VersionMap::new();
        let mut snapshot_mem = vec![0u8; 64];

        let mut store = Vec::new();
        store.push("test 1".to_owned());
        store.push("test 2".to_owned());
        store.push("test 3".to_owned());

        store
            .serialize(&mut snapshot_mem.as_mut_slice(), &vm, 1)
            .unwrap();
        let restore =
            <Vec<String> as Versionize>::deserialize(&mut snapshot_mem.as_slice(), &vm, 1).unwrap();

        assert_eq!(store, restore);
    }

    #[test]
    fn test_ser_de_option() {
        let vm = VersionMap::new();
        let mut snapshot_mem = vec![0u8; 64];
        let mut store = Some("test".to_owned());

        store
            .serialize(&mut snapshot_mem.as_mut_slice(), &vm, 1)
            .unwrap();
        let mut restore =
            <Option<String> as Versionize>::deserialize(&mut snapshot_mem.as_slice(), &vm, 1)
                .unwrap();
        assert_eq!(store, restore);

        // Check that ser_de also works for `None` variant.
        store = None;
        store
            .serialize(&mut snapshot_mem.as_mut_slice(), &vm, 1)
            .unwrap();
        restore = <Option<String> as Versionize>::deserialize(&mut snapshot_mem.as_slice(), &vm, 1)
            .unwrap();
        assert_eq!(store, restore);

        store = Some("test".to_owned());
        store
            .serialize(&mut snapshot_mem.as_mut_slice(), &vm, 1)
            .unwrap();
        // Corrupt `snapshot_mem` by changing the most significant bit to a value different than 0 or 1.
        snapshot_mem[0] = 2;
        let restore_err =
            <Option<String> as Versionize>::deserialize(&mut snapshot_mem.as_slice(), &vm, 1)
                .unwrap_err();
        assert_eq!(
            restore_err,
            VersionizeError::Deserialize("Invalid option value 2".to_string())
        );
        // Corrupt `snapshot_mem` by changing the most significant bit from 1 (`Some(type)`) to 0 (`None`).
        snapshot_mem[0] = 0;
        restore = <Option<String> as Versionize>::deserialize(&mut snapshot_mem.as_slice(), &vm, 1)
            .unwrap();
        assert_ne!(store, restore);
    }

    #[test]
    fn test_ser_de_box() {
        let vm = VersionMap::new();
        let mut snapshot_mem = vec![0u8; 64];

        let store = Box::new("test".to_owned());

        store
            .serialize(&mut snapshot_mem.as_mut_slice(), &vm, 1)
            .unwrap();
        let restore =
            <Box<String> as Versionize>::deserialize(&mut snapshot_mem.as_slice(), &vm, 1).unwrap();

        assert_eq!(store, restore);
    }

    #[test]
    fn test_ser_de_wrapping() {
        let vm = VersionMap::new();
        let mut snapshot_mem = vec![0u8; 64];

        let store = std::num::Wrapping(1337u32);

        store
            .serialize(&mut snapshot_mem.as_mut_slice(), &vm, 1)
            .unwrap();
        let restore = <std::num::Wrapping<u32> as Versionize>::deserialize(
            &mut snapshot_mem.as_slice(),
            &vm,
            1,
        )
        .unwrap();

        assert_eq!(store, restore);
    }

    #[test]
    fn test_vec_limit() {
        // We need extra 8 bytes for vector len.
        let mut snapshot_mem = vec![0u8; MAX_VEC_SIZE + 8];
        let err = vec![123u8; MAX_VEC_SIZE + 1]
            .serialize(&mut snapshot_mem.as_mut_slice(), &VersionMap::new(), 1)
            .unwrap_err();
        assert_eq!(err, VersionizeError::VecLength(MAX_VEC_SIZE + 1));
        assert_eq!(
            format!("{}", err),
            "Vec of length 10485761 exceeded maximum size of 10485760 bytes"
        );
    }

    #[test]
    fn test_string_limit() {
        // We need extra 8 bytes for string len.
        let mut snapshot_mem = vec![0u8; MAX_STRING_LEN + 8];
        let err = String::from_utf8(vec![123u8; MAX_STRING_LEN + 1])
            .unwrap()
            .serialize(&mut snapshot_mem.as_mut_slice(), &VersionMap::new(), 1)
            .unwrap_err();
        assert_eq!(err, VersionizeError::StringLength(MAX_STRING_LEN + 1));
        assert_eq!(
            format!("{}", err),
            "String length exceeded 16385 > 16384 bytes"
        );
    }
}
