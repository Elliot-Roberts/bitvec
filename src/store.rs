#![doc = include_str!("../doc/store.md")]

use core::{cell::Cell, fmt::Debug};

use funty::Integral;

use crate::{
	access::*,
	index::BitIdx,
	mem::{self, BitRegister},
	order::BitOrder,
};

#[doc = include_str!("../doc/store/BitStore.md")]
pub trait BitStore: 'static + Debug {
	/// The element type used in the memory region underlying a `BitSlice`. It
	/// is *always* one of the unsigned integer fundamentals.
	type Mem: BitRegister + BitStore<Mem = Self::Mem>;
	/// A type that selects the appropriate load/store instructions when
	/// accessing the memory bus. It determines what instructions are used when
	/// moving a `Self::Mem` value between the processor and the memory system.
	///
	/// This must be *at least* able to manage aliasing.
	type Access: BitAccess<Item = Self::Mem> + BitStore<Mem = Self::Mem>;
	/// A sibling `BitStore` implementor that is known to be alias-safe. It is
	/// used when a `BitSlice` introduces multiple handles that view the same
	/// memory location, and at least one of them has write capabilities to it.
	/// It must have the same underlying memory type, and can only change access
	/// patterns or public-facing usage.
	type Alias: BitStore<Mem = Self::Mem>;
	/// The inverse of `::Alias`. It is used when a `BitSlice` removes the
	/// conditions that required a `T -> T::Alias` transition.
	type Unalias: BitStore<Mem = Self::Mem>;

	/// The zero constant.
	const ZERO: Self;

	/// Wraps a raw memory value as a `BitStore` type.
	fn new(value: Self::Mem) -> Self;

	/// Loads a value out of the memory system according to the `::Access`
	/// rules. This may be called when the value is aliased by a write-capable
	/// reference.
	fn load_value(&self) -> Self::Mem;

	/// Stores a value into the memory system. This is only called when there
	/// are no other handles to the value, and it may bypass `::Access`
	/// constraints.
	fn store_value(&mut self, value: Self::Mem);

	/// Reads a single bit out of the memory system according to the `::Access`
	/// rules. This is lifted from [`BitAccess`] so that it can be used
	/// elsewhere without additional casts.
	///
	/// ## Type Parameters
	///
	/// - `O`: The ordering of bits within `Self::Mem` governing the lookup.
	///
	/// ## Parameters
	///
	/// - `index`: The semantic index of a bit in `*self`.
	///
	/// ## Returns
	///
	/// The value of the bit in `*self` at `BitOrder::at(index)`.
	///
	/// [`BitAccess`]: crate::access::BitAccess
	#[inline]
	fn get_bit<O>(&self, index: BitIdx<Self::Mem>) -> bool
	where
		O: BitOrder,
	{
		self.load_value() & index.select::<O>().into_inner()
			!= <Self::Mem as Integral>::ZERO
	}

	/// All implementors are required to have their alignment match their size.
	///
	/// Use [`mem::aligned_to_size::<Self>()`][0] to prove this.
	///
	/// [0]: crate::mem::aligned_to_size
	const ALIGNED_TO_SIZE: [(); 1];

	/// All implementors are required to have `Self` and `Self::Alias` be equal
	/// in representation. This is true by fiat for all types except the
	/// unsigned integers.
	///
	/// Use [`mem::layout_eq::<Self, Self::Alias>()`][0] to prove this.
	///
	/// [0]: crate::mem::layout_eq
	const ALIAS_WIDTH: [(); 1];
}

/// Generates `BitStore` implementations for ordinary integers and `Cell`s.
macro_rules! store {
	($($base:ty => $safe:ty);+ $(;)?) => { $(
		impl BitStore for $base {
			type Mem = Self;
			/// The unsigned integers will only be `BitStore` type parameters
			/// for handles to unaliased memory, following the normal Rust
			/// reference rules.
			type Access = Cell<Self>;
			type Alias = $safe;
			type Unalias = Self;

			const ZERO: Self = 0;

			#[inline]
			fn new(value: Self::Mem) -> Self { value }

			#[inline]
			fn load_value(&self) -> Self::Mem {
				*self
			}

			#[inline]
			fn store_value(&mut self, value: Self::Mem) {
				*self = value;
			}

			const ALIGNED_TO_SIZE: [(); 1]
				= [(); mem::aligned_to_size::<Self>() as usize];

			const ALIAS_WIDTH: [(); 1]
				= [(); mem::layout_eq::<Self, Self::Alias>() as usize];
		}

		impl BitStore for $safe {
			type Mem = $base;
			type Access = <Self as BitSafe>::Rad;
			type Alias = Self;
			type Unalias = $base;

			const ZERO: Self = <Self as BitSafe>::ZERO;

			#[inline]
			fn new(value: Self::Mem) -> Self { <Self>::new(value) }

			#[inline]
			fn load_value(&self) -> Self::Mem {
				self.load()
			}

			#[inline]
			fn store_value(&mut self, value: Self::Mem) {
				*self = Self::new(value);
			}

			const ALIGNED_TO_SIZE: [(); 1]
				= [(); mem::aligned_to_size::<Self>() as usize];

			const ALIAS_WIDTH: [(); 1] = [()];
		}

		impl BitStore for Cell<$base> {
			type Mem = $base;
			type Access = Self;
			type Alias = Self;
			type Unalias = Self;

			const ZERO: Self = Self::new(0);

			#[inline]
			fn new(value: Self::Mem) -> Self { <Self>::new(value) }

			#[inline]
			fn load_value(&self) -> Self::Mem {
				self.get()
			}

			#[inline]
			fn store_value(&mut self, value: Self::Mem) {
				*self = Self::new(value);
			}

			const ALIGNED_TO_SIZE: [(); 1]
				= [(); mem::aligned_to_size::<Self>() as usize];

			const ALIAS_WIDTH: [(); 1] = [()];
		}
	)+ };
}

store! {
	u8 => BitSafeU8;
}

/// Generates `BitStore` implementations for atomic types.
macro_rules! atomic {
	($($size:tt, $base:ty => $atom:ident);+ $(;)?) => { $(
		radium::if_atomic!(if atomic($size) {
			use core::sync::atomic::$atom;

			impl BitStore for $atom {
				type Mem = $base;
				type Access = Self;
				type Alias = Self;
				type Unalias = Self;

				const ZERO: Self = <Self>::new(0);

				#[inline]
				fn new(value: Self::Mem) -> Self { <Self>::new(value) }

				#[inline]
				fn load_value(&self) -> Self::Mem {
					self.load(core::sync::atomic::Ordering::Relaxed)
				}

				#[inline]
				fn store_value(&mut self, value: Self::Mem) {
					*self = Self::new(value);
				}

				const ALIGNED_TO_SIZE: [(); 1]
					= [(); mem::aligned_to_size::<Self>() as usize];

				const ALIAS_WIDTH: [(); 1] = [()];
			}
		});
	)+ };
}

atomic! {
	8, u8 => AtomicU8;
}

#[cfg(test)]
mod tests {
	use static_assertions::*;

	use super::*;
	use crate::prelude::*;

	/// Unaliased `BitSlice`s are universally threadsafe, because they satisfy
	/// Rust’s unsynchronized mutation rules.
	#[test]
	fn unaliased_send_sync() {
		assert_impl_all!(BitSlice<u8, LocalBits>: Send, Sync);
	}

	#[test]
	fn cell_unsend_unsync() {
		assert_not_impl_any!(BitSlice<Cell<u8>, LocalBits>: Send, Sync);
	}

	/// In non-atomic builds, aliased `BitSlice`s become universally
	/// thread-unsafe. An `&mut BitSlice` is an `&Cell`, and `&Cell` cannot be
	/// sent across threads.
	///
	/// This test cannot be meaningfully expressed in atomic builds, because the
	/// atomicity of a `BitSafeUN` type is target-specific, and expressed in
	/// `radium` rather than in `bitvec`.
	#[test]
	#[cfg(not(feature = "atomic"))]
	fn aliased_non_atomic_unsend_unsync() {
		assert_not_impl_any!(BitSlice<BitSafeU8, LocalBits>: Send, Sync);
	}

	#[test]
	#[cfg(feature = "atomic")]
	fn aliased_atomic_send_sync() {
		assert_impl_all!(BitSlice<AtomicU8, LocalBits>: Send, Sync);
	}
}
