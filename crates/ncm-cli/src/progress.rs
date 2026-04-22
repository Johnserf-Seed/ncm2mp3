use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub fn build_multi() -> MultiProgress {
    MultiProgress::new()
}

pub fn build_bar(multi: &MultiProgress, total: u64) -> ProgressBar {
    let pb = multi.add(ProgressBar::new(total));
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {wide_msg}",
        )
        .expect("progress template must parse")
        .progress_chars("=> "),
    );
    pb
}
