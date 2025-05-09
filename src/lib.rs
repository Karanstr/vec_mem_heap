#![warn(missing_docs)]
//! Fun little virtual memory allocator.
//! 
//! This is a learning experience for me and should be used with a mountain of salt.
//! At some point I need to rename it, but I don't have a good one yet.
//! 
//! This crate is intended for the creation of graphs and similar data structures, with a focus on storing data contiguously in memory while allowing it to have multiple owners. Internally the data is stored in a [Vec].
//! 
//! This crate does not yet support Weak or Atomic references to data, that's on the todo list (maybe).
//! 
//! Errors will cancel the request and returning an [AccessError].
//! 
//! Indexing is performed internally using [usize], but you can use any type which implements [Indexable].
//! 
//! # Example
//! ```
//! use vec_mem_heap::prelude::*;
//! 
//! fn main() {
//! 
//!     let mut storage : NodeField<u32> = NodeField::new();
//! 
//!     // When you push data into the structure, it returns the index that data was stored at and sets the reference count to 1.
//!     let data1 = storage.push(15); // data1 == 0
//!
//!     {
//!         let data2 = storage.push(72); // data2 == 1
//! 
//!         // Now that a second reference to the data at index 0 exists, we have to manually add to the reference count.
//!         let data3 = data1;
//!         storage.add_ref(data3);
//!     
//!         // data2 and data3 are about to go out of scope, so we have to manually remove their references.
//!         // returns Ok( Some(72) ) -> The data at index 1 only had one reference, so it was freed.
//!         storage.remove_ref(data2);
//! 
//!         // returns Ok( None ) -> The data at index 0 had two references, now one.
//!         storage.remove_ref(data3); 
//!     }
//! 
//!     // returns Ok( &15 ) -> The data at index 0 (data1) still has one reference.
//!     dbg!( storage.data( data1 ) );
//!     // Err( AccessError::FreeMemory(1) ) -> The data at index 1 was freed when its last reference was removed.
//!     dbg!( storage.data( 1 ) );
//! 
//! }
//! ```

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// Common types and traits exported for convenience.
/// 
/// This module re-exports the essential types and traits.
/// Import everything from this module with `use vec_mem_heap::prelude::*`.
pub mod prelude {
    pub use super::{
        NodeField, 
        Indexable,
        AccessError
    };
}

/// A trait which allows you to customize how indexes are stored on your side of the api
pub trait Indexable {
    /// Allows the library to convert your type to its internal [Index] representation (currently [usize])
    fn to_index(&self) -> Index;
}
type Index = usize;
impl Indexable for usize {
    fn to_index(&self) -> Index { *self }
}


/// Errors which may occur while accessing and modifying memory.
#[derive(Debug)]
pub enum AccessError {
    /// Returned when attempting to access an index which isn't currently allocated
    FreeMemory(Index),
    /// Returned when a reference operation causes an over/underflow
    ReferenceOverflow,
}

/// Used to allocate space on the heap, read from that space, and write to it.
#[derive(Serialize, Deserialize, Debug)]
pub struct NodeField<T:Clone> {
    /// List of all data stored within this structure
    data : Vec< Option< T > >,
    /// A reference count for each data slot
    refs : Vec<Option<usize>>,
}

// Private methods
impl<T:Clone> NodeField<T> {
    fn last_index(&self) -> Index {
        self.data.len() - 1
    }

    fn first_free(&self) -> Option<Index> {
        for (index, reference) in self.refs.iter().enumerate() {
            if reference.is_none() { return Some(index) }
        }
        None
    }

    fn mark_free(&mut self, index:Index) {
        self.refs[index] = None;
    }

    fn mark_reserved(&mut self, index:Index) {
        self.refs[index] = Some(0);
    }

    fn release(&mut self, index:Index) -> T {
        if let Some(data) = self.data[index].take() {
            self.mark_free(index);
            data
        } else { panic!("Tried to release a free slot"); }
    }

    #[must_use]
    fn reserve(&mut self) -> Index {
        let index = match self.first_free() {
            Some(index) => index,
            None => {
                self.data.push(None);
                self.refs.push(None);
                self.last_index()
            }
        };
        self.mark_reserved(index);
        index
    }

}

// Public functions
impl<T:Clone> NodeField<T> {
    /// Constructs a new `NodeField` which can store data of type `T` 
    /// # Example
    /// ```
    /// //Stores i32s
    /// let mut storage = NodeField::<i32>::new();
    /// ```
    pub fn new() -> Self {
        Self {
            data : Vec::new(),
            refs : Vec::new(),
        }
    }

    /// Returns an immutable reference to the data stored at the requested index, or an [AccessError] if there is a problem.
    pub fn get<I:Indexable>(&self, index:I) -> Result<&T, AccessError> {
        if let Some(data) = self.data.get(index.to_index()) {
            Ok(data.as_ref().unwrap())
        } else { Err(AccessError::FreeMemory(index.to_index())) }
    }

    /// Returns a mutable reference to the data stored at the requested index, or an [AccessError] if there is a problem.
    pub fn get_mut<I:Indexable>(&mut self, index:I) -> Result<&mut T, AccessError> {
        if let Some(data) = self.data.get_mut(index.to_index()) {
            Ok(data.as_mut().unwrap())
        } else { Err(AccessError::FreeMemory(index.to_index())) }
    }

    /// Tells the NodeField that something else references the data at `index`.
    /// So long as the NodeField thinks there is at least one reference, the data won't be freed.
    /// 
    /// Failure to properly track references will lead to either freeing data you wanted or leaking data you didn't.
    pub fn add_ref<I:Indexable>(&mut self, index:I) -> Result<(), AccessError> {
        if let Some(Some(count)) = self.refs.get_mut(index.to_index()) {
            *count = count.checked_add(1).ok_or(AccessError::ReferenceOverflow)?;
            Ok(())
        } else { Err(AccessError::FreeMemory(index.to_index())) }
    }

    /// Tells the NodeField that something no longer references the data at `index`.
    /// If calling this function renders the refcount 0 the data will be freed and returned.
    /// 
    /// Failure to properly track references will lead to either freeing data you wanted or leaking data you didn't.
    pub fn remove_ref<I:Indexable>(&mut self, index:I) -> Result<Option<T>, AccessError> {
        let internal_index = index.to_index();
        if let Some(Some(count)) = self.refs.get_mut(internal_index) {
            *count = count.checked_sub(1).ok_or(AccessError::ReferenceOverflow)?;
            if *count == 0 { Ok( Some( self.release(internal_index) ) ) } else { Ok(None) }
        } else { Err(AccessError::FreeMemory(internal_index)) }
    }

    /// Returns the number of references the data at `index` has or an [AccessError] if there is a problem.
    pub fn status<I:Indexable>(&self, index:I) -> Result<usize, AccessError> {
        if let Some(Some(count)) = self.refs.get(index.to_index()) {
            Ok(*count)
        } else { Err(AccessError::FreeMemory(index.to_index())) }
    }

    /// Pushes `data` into the NodeField, returning the index it was stored at.
    /// 
    /// Once you recieve the index the data was stored at, it is your responsibility to manage its references.
    /// The data will start with one reference.
    #[must_use]
    pub fn push(&mut self, data:T) -> Index {
        let index = self.reserve();
        self.data[index] = Some(data);
        self.add_ref(index).unwrap();
        index
    }

    /// Replaces the data at `index` with `new_data`, returning the original data on success and an [AccessError] on failure.
    /// You may not replace an index which is currently free. 
    pub fn replace<I:Indexable>(&mut self, index:I, new_data:T) -> Result<T, AccessError> {
        if let Some(Some(_)) = self.refs.get(index.to_index()) {
            Ok(self.data[index.to_index()].replace(new_data).unwrap())
        } else { Err(AccessError::FreeMemory(index.to_index())) }
    }

    /// Returns the next index which will be allocated on a [NodeField::push] call
    pub fn next_allocated(&self) -> Index { 
        self.first_free().unwrap_or(self.data.len())
    }

    /// Travels through memory and re-arranges slots so that they are contiguous in memory, with no free slots in between occupied ones.
    /// The hashmap returned can be used to remap your references to their new locations. (Key:Old, Value:New)
    /// 
    /// Slots at the back of memory will be placed in the first free slot, until the above condition is met.
    /// 
    /// This operation is O(n) to the number of slots in memory.
    #[must_use]
    pub fn defrag(&mut self) -> HashMap<Index, Index> {
        let mut remapped = HashMap::new();
        let mut solid_until = 0;
        if solid_until == self.data.len() { return remapped }
        let mut free_until = self.data.len() - 1;
        'defrag: loop {
            while let Some(_) = self.data[solid_until] { 
                solid_until += 1;
                if solid_until == free_until { break 'defrag }
            }
            while let None = self.data[free_until] { 
                free_until -= 1;
                if free_until == solid_until { break 'defrag }
            }
            remapped.insert(free_until, solid_until);
            self.data.swap(free_until, solid_until);
            self.refs.swap(free_until, solid_until);
        }
        remapped
    }

    /// [NodeField::defrag]s the memory, then shrinks the internal memory Vec to the size of the block of occupied memory.
    #[must_use]
    pub fn trim(&mut self) -> HashMap<Index, Index> {
        let remap = self.defrag();
        if let Some(first_free) = self.first_free() {
            self.data.truncate(first_free);
            self.data.shrink_to_fit();
            self.refs.truncate(first_free);
            self.refs.shrink_to_fit();
        }
        remap
    }

    /// Returns a reference to the internal data Vec
    pub fn data(&self) -> &Vec< Option< T > > {
        &self.data
    }

    /// Returns a reference to the internal reference Vec
    pub fn refs(&self) -> &Vec< Option< usize > > {
        &self.refs
    }
}
