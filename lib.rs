//
// multilist/lib.rs
//
// Copyright (c) 2015 Mozilla Foundation
//

#![feature(alloc, core, unsafe_destructor)]

extern crate alloc;

use alloc::heap;
use std::cell::UnsafeCell;
use std::iter;
use std::mem;
use std::ops::Deref;
use std::ptr;

/// An intrusive set of doubly-linked lists, indexed by number. Objects owned by the multilist can
/// belong to any number of the constituent linked lists. Only one allocation is used per object.
///
/// When first adding an object (e.g. via `push_back()`), you choose which linked list it is to
/// initially belong to. You can then find it with an iterator and add it to other lists via
/// `push_back_existing()`. Objects can be removed from individual lists with `remove_existing()`
/// and removed from the list entirely with `pop_back()`. You can iterate over linked lists with
/// `iter()`. When the multilist is destroyed, all objects within it are destroyed as well; in
/// this way, the lists *collectively own* the objects.
///
/// Objects owned by the multilist are normally immutable, but you can use `Cell` or `RefCell` as
/// usual to make their fields mutable. `multilist` is believed to be a memory-safe design,
/// although it is possible to leak with incorrect use of `remove_existing()`. Fixing this would
/// require reference counting the list items.
pub struct Multilist<Value> {
    pointers: UnsafeCell<Vec<MultilistListPointers<Value>>>,
}

#[unsafe_destructor]
impl<Value> Drop for Multilist<Value> {
    fn drop(&mut self) {
        for i in range(0, self.list_count()) {
            while self.pop_back(i).is_some() {}
        }
    }
}

impl<Value> Multilist<Value> {
    #[inline]
    pub fn new(list_count: usize) -> Multilist<Value> {
        Multilist {
            pointers: UnsafeCell::new(iter::repeat(MultilistListPointers::new()).take(list_count as
                                                                                      usize)
                                                                                .collect()),
        }
    }

    #[inline]
    pub fn list_count(&self) -> usize {
        unsafe {
            (*self.pointers.get()).len()
        }
    }

    #[inline]
    pub fn is_empty(&self, list_index: usize) -> bool {
        unsafe {
            (*self.pointers.get())[list_index].head.is_null()
        }
    }

    /// Inserts a brand-new element into one of the lists.
    #[inline]
    pub fn push_back(&self, list_index: usize, value: Value) {
        let element = MultilistElement::new(value, self);
        self.push_back_existing(list_index, element);
    }

    /// Inserts an element that is already in at least one of the lists into another list.
    #[inline]
    pub fn push_back_existing(&self, list_index: usize, element: MultilistElement<Value>) {
        unsafe {
            assert!(element.associated_multilist() == self as *const _);
            let pointers = element.pointers(list_index);
            assert!((*pointers).next.is_null());
            debug_assert!((*pointers).prev.is_null());
            let list_pointers = &mut (*self.pointers.get())[list_index];
            if list_pointers.tail.is_null() {
                list_pointers.head = element.holder as *mut _;
            } else {
                (*(*list_pointers.tail).pointers(list_index)).next = element.holder as *mut _;
                (*pointers).prev = list_pointers.tail;
            }
            list_pointers.tail = element.holder as *mut _;
        }
    }

    /// Removes an element from one of the lists.
    ///
    /// NB: If the element is no longer a member of any lists, this will leak the element! You
    /// should use `pop_back()` to remove the element from the last list it's a member of.
    #[inline]
    pub fn remove_existing(&self, list_index: usize, element: MultilistElement<Value>) {
        unsafe {
            assert!(element.associated_multilist() == self as *const _);
            let pointers = element.pointers(list_index);
            let list_pointers = &mut (*self.pointers.get())[list_index];
            if (*pointers).next.is_null() {
                // Make sure it's actually in the list!
                assert!(list_pointers.tail == element.holder as *mut _);

                list_pointers.tail = ptr::null_mut();
            } else {
                (*((*(*pointers).next)).pointers(list_index)).prev = (*pointers).prev;
            }
            if (*element.pointers(list_index)).prev.is_null() {
                list_pointers.head = (*element.pointers(list_index)).next;
            } else {
                (*((*(*pointers).prev)).pointers(list_index)).next = (*pointers).next;
            }
        }
    }

    /// Removes the last element of the given list from all of the lists it's a member of and
    /// returns it.
    #[inline]
    pub fn pop_back(&mut self, list_index: usize) -> Option<Value> {
        unsafe {
            let tail = (*self.pointers.get())[list_index].tail;
            let mut element = if !tail.is_null() {
                MultilistElement {
                    holder: tail,
                }
            } else {
                return None
            };
            for i in range(0, self.list_count()) {
                if element.is_in_list(i) {
                    self.remove_existing(i, element)
                }
            }
            let value = ptr::read(&(*element.holder).value);
            element.destroy();
            Some(value)
        }
    }

    /// Iterates over one of the linked lists.
    #[inline]
    pub fn iter<'a>(&'a self, list_index: usize) -> MultilistIterator<'a,Value> {
        unsafe {
            MultilistIterator {
                element: (*self.pointers.get())[list_index].head,
                list_index: list_index,
            }
        }
    }
}

struct MultilistElementHolder<Value> {
    value: Value,
    associated_multilist: *const Multilist<Value>,
    pointers: UnsafeCell<[MultilistPointers<Value>; 1]>,
}

impl<Value> MultilistElementHolder<Value> {
    fn size(list_count: usize) -> usize {
        debug_assert!(list_count > 0);
        mem::size_of::<MultilistElementHolder<Value>>() +
            (mem::min_align_of::<MultilistPointers<Value>>() * (list_count - 1) as usize)

    }

    #[inline]
    fn pointers(&self, list_index: usize) -> *mut MultilistPointers<Value> {
        unsafe {
            debug_assert!(list_index < (*self.associated_multilist).list_count());
            (*self.pointers.get()).as_ptr().offset(list_index as isize) as
                *mut MultilistPointers<Value>
        }
    }
}

/// One element in a multilist.
pub struct MultilistElement<'a,Value> {
    holder: *const MultilistElementHolder<Value>,
}

impl<'a,Value> Copy for MultilistElement<'a,Value> {}

impl<'a,Value> Clone for MultilistElement<'a,Value> {
    fn clone(&self) -> MultilistElement<'a,Value> {
        *self
    }
}

impl<'a,Value> Deref for MultilistElement<'a,Value> {
    type Target = Value;

    #[inline]
    fn deref<'b>(&'b self) -> &'b Value {
        unsafe {
            &(*self.holder).value
        }
    }
}

impl<'a,Value> MultilistElement<'a,Value> {
    #[inline]
    fn new(value: Value, associated_multilist: &'a Multilist<Value>)
           -> MultilistElement<'a,Value> {
        unsafe {
            let byte_size =
                MultilistElementHolder::<Value>::size((*associated_multilist.pointers
                                                                            .get()).len());
            let holder = heap::allocate(byte_size, byte_size) as
                *mut MultilistElementHolder<Value>;
            if holder.is_null() {
                alloc::oom()
            }
            ptr::write(holder, MultilistElementHolder {
                value: value,
                associated_multilist: associated_multilist,
                pointers: UnsafeCell::new([MultilistPointers::new()]),
            });
            for i in range(mem::size_of::<MultilistElement<Value>>(), byte_size) {
                ptr::write((*(*holder).pointers.get()).as_mut_ptr().offset(i as isize),
                           MultilistPointers::new())
            }
            MultilistElement {
                holder: holder,
            }
        }
    }

    #[inline]
    unsafe fn pointers(&self, list_index: usize) -> *mut MultilistPointers<Value> {
        (*self.holder).pointers(list_index)
    }

    #[inline]
    fn associated_multilist(&self) -> *const Multilist<Value> {
        unsafe {
            (*self.holder).associated_multilist
        }
    }

    #[inline]
    unsafe fn destroy(&mut self) {
        let byte_size =
            MultilistElementHolder::<Value>::size((*(*self.associated_multilist()).pointers
                                                                                  .get()).len());
        drop(heap::deallocate(self.holder as *mut u8, byte_size, byte_size))
    }

    /// Returns true if this element is a member of the given list.
    #[inline]
    pub fn is_in_list(&self, list_index: usize) -> bool {
        unsafe {
            let pointers = self.pointers(list_index);
            !(*pointers).next.is_null() || !(*pointers).prev.is_null()
        }
    }
}

pub struct MultilistPointers<Value> {
    next: *mut MultilistElementHolder<Value>,
    prev: *mut MultilistElementHolder<Value>,
}

impl<Value> Copy for MultilistPointers<Value> {}

impl<Value> Clone for MultilistPointers<Value> {
    fn clone(&self) -> MultilistPointers<Value> {
        *self
    }
}

impl<Value> MultilistPointers<Value> {
    pub fn new() -> MultilistPointers<Value> {
        MultilistPointers {
            next: ptr::null_mut(),
            prev: ptr::null_mut(),
        }
    }
}

pub struct MultilistListPointers<Value> {
    head: *mut MultilistElementHolder<Value>,
    tail: *mut MultilistElementHolder<Value>,
}

impl<Value> Copy for MultilistListPointers<Value> {}

impl<Value> Clone for MultilistListPointers<Value> {
    fn clone(&self) -> MultilistListPointers<Value> {
        *self
    }
}

impl<Value> MultilistListPointers<Value> {
    pub fn new() -> MultilistListPointers<Value> {
        MultilistListPointers {
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
        }
    }
}

pub struct MultilistIterator<'a,Value> {
    element: *mut MultilistElementHolder<Value>,
    list_index: usize,
}

impl<'a,Value> Iterator for MultilistIterator<'a,Value> {
    type Item = MultilistElement<'a,Value>;

    fn next(&mut self) -> Option<MultilistElement<'a,Value>> {
        let element = self.element;
        if element.is_null() {
            return None
        }

        unsafe {
            let next = (*(*element).pointers(self.list_index)).next;
            self.element = next;
            Some(MultilistElement {
                holder: element,
            })
        }
    }
}

/// Example code. This is skeleton code that shows how this might be used in an operating system
/// kernel to manage tasks.
#[allow(dead_code)]
fn main() {
    #[derive(Debug)]
    struct TaskStruct {
        pid: i32,
        gid: i32,
    }

    const TASK_LIST: usize = 0;
    const RUN_LIST: usize = 1;

    let mut multilist = Multilist::new(2);
    multilist.push_back(TASK_LIST, TaskStruct {
        pid: 1,
        gid: 2,
    });
    multilist.push_back(TASK_LIST, TaskStruct {
        pid: 3,
        gid: 4,
    });
    multilist.push_back(TASK_LIST, TaskStruct {
        pid: 5,
        gid: 6,
    });
    println!("After adding 3 tasks to task list:");
    dump_list(&multilist);

    multilist.push_back_existing(RUN_LIST, multilist.iter(TASK_LIST).skip(2).next().unwrap());
    multilist.push_back_existing(RUN_LIST, multilist.iter(TASK_LIST).skip(0).next().unwrap());
    multilist.push_back_existing(RUN_LIST, multilist.iter(TASK_LIST).skip(1).next().unwrap());
    println!("\nAfter adding 3 tasks to run list in order 2, 0, 1:");
    dump_list(&multilist);

    multilist.remove_existing(TASK_LIST, multilist.iter(TASK_LIST).skip(1).next().unwrap());
    println!("\nAfter removing the second task from the task list:");
    dump_list(&multilist);

    multilist.push_back(TASK_LIST, TaskStruct {
        pid: 7,
        gid: 8,
    });
    println!("\nAfter adding a new task to the task list:");
    dump_list(&multilist);

    multilist.pop_back(RUN_LIST);
    println!("\nAfter removing the last task on the run list entirely:");
    dump_list(&multilist);

    return;

    fn dump_list(multilist: &Multilist<TaskStruct>) {
        println!("Tasks in task order:");
        for task in multilist.iter(TASK_LIST) {
            println!("{:?}", &*task);
        }
        println!("Tasks in run order:");
        for task in multilist.iter(RUN_LIST) {
            println!("{:?}", &*task);
        }
    }
}

