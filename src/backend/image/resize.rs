pub fn fit_dimensions(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    let longest_edge = width.max(height);

    // 小图不放大
    if longest_edge <= max_edge {
        return (width, height);
    }

    if width >= height {
        (max_edge, scale_dimension(height, max_edge, width))
    } else {
        (scale_dimension(width, max_edge, height), max_edge)
    }
}

fn scale_dimension(value: u32, numerator: u32, denominator: u32) -> u32 {
    let value = u64::from(value);
    let numerator = u64::from(numerator);
    let denominator = u64::from(denominator);

    // 加denominator/2,使整数除法接近四舍五入
    let result = (value * numerator + denominator / 2) / denominator;

    u32::try_from(result.max(1)).expect("缩放后的尺寸不会超过max_edge")
}
