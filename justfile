set positional-arguments

# format all (with some special nightly only options that aren't strictly enforced but recommended)
fmt:
    cargo +nightly fmt -- --config group_imports=StdExternalCrate,imports_granularity=Module
    cargo fmt --all

# print ffmpeg dr score for provided file
ffmpeg file:
    ffmpeg -hide_banner -nostats -i $1 -filter:a drmeter -f null -

# print drmeter dr score for provided file
run file:
    cargo run --release --example drmeter -- $1
