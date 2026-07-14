//! 台形補正（透視変換）。検出四隅を 4:3 または 16:9 の矩形へ写像する。
//! Python 版 warp.py の移植。画角外は黒(0)で埋める。

use image::RgbImage;
use imageproc::geometric_transformations::{warp_into, Interpolation, Projection};

use crate::geometry as geo;

pub fn warp_to_rect(img: &RgbImage, quad: &geo::Quad, max_long_side: u32, margin: f64) -> RgbImage {
    let q = geo::order_corners(quad);
    let (w_img, h_img) = img.dimensions();

    // 出力比は Zhang-He 透視補正＋確度ゲートで 4:3/16:9 を決める
    let aspect = geo::decide_output_aspect(&q, (w_img, h_img));

    let s = geo::side_lengths(&q);
    let width_px = s[0].max(s[2]);
    let height_px = s[1].max(s[3]);
    let base = (width_px * height_px).max(1.0).sqrt();
    let mut out_w = base * aspect.sqrt();
    let mut out_h = base / aspect.sqrt();
    let scale = max_long_side as f64 / out_w.max(out_h);
    if scale < 1.0 {
        out_w *= scale;
        out_h *= scale;
    }
    let out_w = out_w.round().max(1.0) as u32;
    let out_h = out_h.round().max(1.0) as u32;

    // dst 矩形の四隅（TL,TR,BR,BL）
    let dst = [
        (0.0f32, 0.0f32),
        ((out_w - 1) as f32, 0.0),
        ((out_w - 1) as f32, (out_h - 1) as f32),
        (0.0, (out_h - 1) as f32),
    ];
    // マージン: 重心から等倍拡大して各辺を margin だけ外へ広げた quad を src に使う
    // （出力矩形は据え置きなのでスライドが内側に縮み、周辺マージンが入る。トリミング後の
    // 画像だけでスライド全体が収まっているか判断しやすくする）。
    let cx = (q[0][0] + q[1][0] + q[2][0] + q[3][0]) / 4.0;
    let cy = (q[0][1] + q[1][1] + q[2][1] + q[3][1]) / 4.0;
    let f = 1.0 + 2.0 * margin;
    let ex = |p: geo::Point| (cx + (p[0] - cx) * f, cy + (p[1] - cy) * f);
    let (s0, s1, s2, s3) = (ex(q[0]), ex(q[1]), ex(q[2]), ex(q[3]));
    let src = [
        (s0.0 as f32, s0.1 as f32),
        (s1.0 as f32, s1.1 as f32),
        (s2.0 as f32, s2.1 as f32),
        (s3.0 as f32, s3.1 as f32),
    ];

    // imageproc の warp は「入力→出力」の射影を受け取り内部で逆写像してサンプルする。
    // よって src(スライド四隅)→dst(出力矩形) の順で射影を作る。
    let proj = match Projection::from_control_points(src, dst) {
        Some(p) => p,
        None => return img.clone(),
    };
    let mut out = RgbImage::new(out_w, out_h);
    warp_into(
        img,
        &proj,
        Interpolation::Bicubic,
        image::Rgb([0, 0, 0]),
        &mut out,
    );
    out
}
