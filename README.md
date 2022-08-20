# drmeter [![crates.io](https://img.shields.io/crates/v/drmeter.svg)](https://crates.io/crates/drmeter) [![Actions Status](https://github.com/sagudev/drmeter/workflows/CI/badge.svg)](https://github.com/sagudev/drmeter/actions) [![docs.rs](https://docs.rs/drmeter/badge.svg)](https://docs.rs/drmeter)

Implementation of the [TT DR Meter](https://web.archive.org/web/20180917133436/http://www.dynamicrange.de/sites/default/files/Measuring%20DR%20ENv3.pdf).

This crate provides an API which analyzes audio and outputs DR score. The results
are nearly compatible with TT DR Offline.

This crate is a Rust port of FFMPEG's [libavfilter/drmeter](https://github.com/FFmpeg/FFmpeg/blob/master/libavfilter/af_drmeter.c). A lot of inspiration especially around samples handling was taken from [ebur128](https://github.com/sdroege/ebur128).
