`multilist` is a Rust data structure that represents an *intrusive set of doubly-linked lists*, indexed by
number. Objects owned by the multilist can belong to any number of the constituent linked lists. Only one
allocation is used per object, regardless of the number of lists it is in.

When first adding an object (e.g. via `push_back()`), you choose which linked list it is to initially belong
to. You can then find it with an iterator and add it to other lists via `push_back_existing()`. Objects can
be removed from individual lists with `remove_existing()` and removed from the list entirely with
`pop_back()`. You can iterate over linked lists with `iter()`. When the multilist is destroyed, all objects
within it are destroyed as well; in this way, the lists *collectively own* the objects.

Objects owned by the multilist are normally immutable, but you can use `Cell` or `RefCell` as usual to make their fields mutable. `multilist` is believed to be a memory-safe design, although it is possible to leak with incorrect use of `remove_existing()`. Fixing this would require reference counting the list items.

Example code is provided inside `lib.rs`.

`multilist` is distributed under the same terms as the Rust compiler itself.
