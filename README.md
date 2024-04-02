Basic fuzzer to fuzz objdump with a lot of binaries using multiple threads. The thread count can be controlled. By default it uses 4 threads.

All the binaries are to be loaded in a 'corpus' directory in the repo root. Get the binaries from any `/usr/bin` type directory and keep them in the corpus directory.
