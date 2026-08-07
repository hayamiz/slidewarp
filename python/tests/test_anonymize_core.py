import numpy as np
import anonymize_core as ac


def test_strip_and_write_produces_no_exif(tmp_path):
    from PIL import Image

    # EXIF(Orientation)付きの元 JPEG を用意
    src = tmp_path / "src.jpg"
    im = Image.new("RGB", (20, 16), (120, 30, 30))
    exif = im.getexif()
    exif[0x0112] = 6  # Orientation タグ
    im.save(src, exif=exif)
    assert len(Image.open(src).getexif()) > 0  # 前提: 元には EXIF がある

    # strip_and_write で書き出すと EXIF が消える
    import cv2

    out = tmp_path / "out.jpg"
    ac.strip_and_write(cv2.imread(str(src)), out)
    assert out.exists()
    assert len(Image.open(out).getexif()) == 0


def test_inset_quad_shrinks_toward_centroid():
    quad = np.array([[0, 0], [100, 0], [100, 100], [0, 100]], dtype=np.float32)
    out = ac.inset_quad(quad, 0.1)
    # 重心(50,50)へ各頂点が10%寄る -> 角は(10,10),(90,10),(90,90),(10,90)
    expected = np.array([[10, 10], [90, 10], [90, 90], [10, 90]], dtype=np.float32)
    assert np.allclose(out, expected, atol=1e-4)
    # 元の quad は破壊しない
    assert quad[0, 0] == 0


def test_pixelate_blocks_reduce_detail():
    img = np.arange(64 * 64 * 3, dtype=np.uint8).reshape(64, 64, 3)
    out = ac.pixelate(img, block=16)  # 64/16=4x4 に縮小して戻す
    assert out.shape == img.shape
    # 左上 16x16 ブロックは単一色になる（ブロック内が一定）
    block = out[0:16, 0:16].reshape(-1, 3)
    assert np.all(block == block[0])


def test_build_mask_covers_quad_not_edges():
    mask = ac.build_mosaic_mask((100, 100),
                                np.array([[20, 20], [80, 20], [80, 80], [20, 80]], np.float32),
                                [])
    assert mask.shape == (100, 100)
    assert mask[50, 50] == 255      # 内側は対象
    assert mask[0, 0] == 0          # 画像端(辺の外)は非対象


def test_build_mask_adds_face_rects():
    mask = ac.build_mosaic_mask((100, 100), None, [(10, 10, 20, 20)])
    assert mask[15, 15] == 255
    assert mask[50, 50] == 0


def test_apply_mosaic_only_inside_mask():
    img = np.zeros((32, 32, 3), np.uint8)
    img[:] = 100
    img[8:24, 8:24] = np.arange(16 * 16 * 3, dtype=np.uint8).reshape(16, 16, 3)
    mask = np.zeros((32, 32), np.uint8)
    mask[8:24, 8:24] = 255
    out = ac.apply_mosaic(img, mask, block=8)
    assert np.all(out[0, 0] == 100)          # マスク外は不変
    assert not np.array_equal(out[8:24, 8:24], img[8:24, 8:24])  # マスク内は変化


def test_apply_blur_only_inside_mask():
    img = np.zeros((40, 40, 3), np.uint8)
    img[:] = 100
    # マスク内に高周波パターン（市松）を置き、ぼかしで平滑化されることを確認
    img[10:30, 10:30] = (np.indices((20, 20)).sum(0) % 2)[..., None].astype(np.uint8) * 255
    mask = np.zeros((40, 40), np.uint8)
    mask[10:30, 10:30] = 255
    out = ac.apply_blur(img, mask, sigma=5)
    assert np.all(out[0, 0] == 100)                       # マスク外は不変
    assert not np.array_equal(out[10:30, 10:30], img[10:30, 10:30])  # マスク内は変化
    # ぼかし後は市松の高周波が減る（分散が下がる）
    assert out[10:30, 10:30].var() < img[10:30, 10:30].var()


def test_face_bands_top_of_person():
    m = np.zeros((300, 300), np.uint8)
    m[50:250, 100:180] = 255   # 高さ200,幅80 の人物塊（面積>min）
    rects = ac.face_bands_from_person_mask(m, band=0.25, min_area=400)
    assert len(rects) == 1
    x, y, w, h = rects[0]
    assert (x, y, w) == (100, 50, 80)
    assert h == int(200 * 0.25)   # 上部25% = 50px


def test_face_bands_skips_small_blobs():
    m = np.zeros((100, 100), np.uint8)
    m[10:15, 10:15] = 255         # 面積25 < min_area
    assert ac.face_bands_from_person_mask(m, band=0.25, min_area=400) == []


def test_parse_dump_geom_with_spaced_name():
    line = ("hough est=1.330 rec=Some(1.78) persp=Some(0.05) [16:9] "
            "quad=(10,20) (500,25) (505,400) (12,395)  2026-06-23 08.44.43.jpg")
    d = ac.parse_dump_geom(line + "\nnone  weird name.jpg\n")
    g = d["2026-06-23 08.44.43.jpg"]
    assert g.method == "hough"
    assert g.aspect == "16:9"
    assert g.quad.shape == (4, 2)
    assert list(g.quad[0]) == [10, 20]
    assert list(g.quad[2]) == [505, 400]
    assert d["weird name.jpg"].quad is None
    assert d["weird name.jpg"].method == "none"


def test_parse_confidence_with_spaced_name():
    out = "対象 2 枚 / 並列 4\n[OK  ] 2026-06-23 08.44.43.jpg  conf=0.87 hough\n[LOW ] x.jpg  conf=0.20 minrect\n"
    c = ac.parse_confidence(out)
    assert c["2026-06-23 08.44.43.jpg"] == 0.87
    assert c["x.jpg"] == 0.20


def _dg(method="hough", aspect="16:9", quad=((0, 0), (100, 0), (100, 60), (0, 60))):
    return ac.DumpGeom(method, aspect, None if quad is None else np.array(quad, np.float32))


def test_compare_pass_when_identical():
    r = ac.compare(_dg(), _dg(), 0.80, 0.80, img_long_side=1000)
    assert r.passed
    assert r.reasons == []


def test_compare_fail_on_quad_drift():
    moved = _dg(quad=((0, 0), (100, 0), (100, 60), (0, 90)))  # 隅が30pxずれ
    r = ac.compare(_dg(), moved, 0.80, 0.80, img_long_side=1000)  # tol=10px
    assert not r.passed
    assert any("quad" in x for x in r.reasons)
    assert r.quad_drift_px >= 29


def test_compare_fail_on_method_and_conf():
    r = ac.compare(_dg(method="hough"), _dg(method="contour"), 0.80, 0.70, img_long_side=1000)
    assert not r.passed
    assert any("method" in x for x in r.reasons)
    assert any("conf" in x for x in r.reasons)


def test_compare_fail_on_missing_quad():
    r = ac.compare(_dg(), _dg(quad=None), 0.8, 0.8, img_long_side=1000)
    assert not r.passed


def test_write_review_html(tmp_path):
    entries = [{
        "name": "a.jpg",
        "orig_rel": "../../input-samples/a.jpg",
        "anon_rel": "a.jpg",
        "verify": ac.VerifyResult(passed=True, reasons=[], quad_drift_px=1.2, conf_delta=0.01),
    }, {
        "name": "b.jpg",
        "orig_rel": "../../input-samples/b.jpg",
        "anon_rel": "b.jpg",
        "verify": ac.VerifyResult(passed=False, reasons=["quad ズレ 30px"], quad_drift_px=30.0),
    }]
    out = tmp_path / "review.html"
    ac.write_review_html(entries, out)
    html = out.read_text(encoding="utf-8")
    assert "a.jpg" in html and "b.jpg" in html
    assert "PASS" in html and "FAIL" in html
    assert "quad ズレ 30px" in html
    assert "../../input-samples/a.jpg" in html
