# mnemosyne

Utility crate for memoizing return-values of a frequently-called function for a set period of time.

This crate provides the `mnemosyne` library, defining the type `Mnemosyne<T, E>`, which is given
a callback and an update interval upon creation and returns either a memoized or freshly-computed
value when polled, depending on if a full update interval has passed since the last poll. If not,
the previous value is returned without further computation, and otherwise, the callback is invoked
and its return value memoized and yielded to the caller.

This is especially useful for functions whose return value represents a continous real-world metric,
which needs to be tracked over the course of minutes but not seconds; or seconds but not milliseconds.
The `Mnemosyne` type eliminates such time-related book-keeping, and computes the callback value only when
polled (strict, but not eager, evaluation). This is especially useful for API calls that are subject to
rate or frequency limits to prevent DDoS or manage scalability.
