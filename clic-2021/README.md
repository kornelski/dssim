# Test tool for compression.cc

https://github.com/fab-jul/clic2021-devkit/blob/main/README.md#perceptual-challenge

## Usage

Download `clic_2021_perceptual_valid.zip` and decompress it to `clic_2021_perceptual_valid/` dir.

Run the `clic-2021` binary. It expects path to CSV as the first argument:

```rust
cargo run --release clic_2021_perceptual_valid/validation.csv
```

It will create `dssim3.csv` in the current directory.

```bash
cd clic_2021_perceptual_valid
python3 eval_csv.py --oracle_csv=oracle.csv --eval_csv=../dssim3.csv
```

