import numpy as np
import anonymize_core as ac


def test_inset_quad_shrinks_toward_centroid():
    quad = np.array([[0, 0], [100, 0], [100, 100], [0, 100]], dtype=np.float32)
    out = ac.inset_quad(quad, 0.1)
    # 重心(50,50)へ各頂点が10%寄る -> 角は(10,10),(90,10),(90,90),(10,90)
    expected = np.array([[10, 10], [90, 10], [90, 90], [10, 90]], dtype=np.float32)
    assert np.allclose(out, expected, atol=1e-4)
    # 元の quad は破壊しない
    assert quad[0, 0] == 0
