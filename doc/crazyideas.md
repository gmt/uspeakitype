# crazyideas

This branch is an architecture sandbox for rough-draft answers to a now-serious
question:

How much of `usit` should remain a Rust application with a graphical shell
attached, and how much should move toward a desktop-native integration layer?

The experiments under [`experiments/`](/home/greg/src/usit/experiments) are not
meant to converge on production code directly. They are here to make a few
different futures tangible enough to compare:

- C++ shell + Rust helper
- more C++ ownership of audio/visualization
- shared-memory bridge instead of text protocol
- in-process Qt/Rust via CXX-Qt
- in-process Qt/Rust via raw interop

The value of this branch is not elegance. It is comparative ugliness in small,
controlled doses.
