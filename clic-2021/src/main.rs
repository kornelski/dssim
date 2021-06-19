use anyhow::{anyhow, Result, Context};
use concurrent_lru::unsharded::LruCache;
use dssim_core::Dssim;
use dssim_core::DssimImage;
use rayon::prelude::*;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

type AImage = Arc<DssimImage<f32>>;

fn decode_png(dssim: &Dssim, path: PathBuf, cache: &LruCache<PathBuf, AImage>) -> Result<AImage> {
    let img = cache.get_or_try_init(path, 1, |path| {
        let png = lodepng::decode24_file(&path).with_context(|| format!("Can't read {}", path.display()))?;
        let img = dssim.create_image_rgb(&png.buffer, png.width, png.height).ok_or(anyhow!("dssim fail"))?;
        Ok::<_, anyhow::Error>(Arc::new(img))
    })?;

    Ok(img.value().clone())
}

fn main() -> Result<()> {
    let csv_path = std::env::args_os().nth(1).map(PathBuf::from)
        .unwrap_or("clic_2021_perceptual_valid/validation.csv".into());
    let base_dir = csv_path.parent().unwrap_or(Path::new(""));

    let cache = LruCache::new(30);

    let mut validation_csv = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(&csv_path)?;

    let results = validation_csv.records().par_bridge().map(|row| {
        let row = row?;
        let dssim = Dssim::new();
        let o_file_name = row.get(0).ok_or(anyhow!("bad csv"))?;
        let a_file_name = row.get(1).ok_or(anyhow!("bad csv"))?;
        let b_file_name = row.get(2).ok_or(anyhow!("bad csv"))?;
        let o_img = decode_png(&dssim, base_dir.join(o_file_name), &cache)?;
        let a_img = decode_png(&dssim, base_dir.join(a_file_name), &cache)?;
        let b_img = decode_png(&dssim, base_dir.join(b_file_name), &cache)?;

        let a = dssim.compare(&o_img, &*a_img).0;
        let b = dssim.compare(&o_img, &*b_img).0;

        writeln!(std::io::stdout().lock(), "{},{},{} = {} {}", o_file_name, a_file_name, b_file_name, a, b)?;

        Ok((o_file_name.to_owned(), a_file_name.to_owned(), b_file_name.to_owned(), a, b))
    })
    .collect::<Vec<Result<_>>>();

    let mut out = csv::Writer::from_path("dssim3.csv")?;
    for r in results {
        let (o_name, a_name, b_name, a, b) = r?;
        out.write_record(&[o_name.as_str(), a_name.as_str(), b_name.as_str(), if a < b {"0"} else {"1"}])?;
    }

    Ok(())
}
