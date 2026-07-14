"""ML によるスクリーン領域セグメンテーション（ONNX Runtime、任意・プラガブル）。

classical 検出（slidewarp.detect）だけではオクルージョン・はみ出しに弱い場合がある。
ここではセグメンテーションモデルでスクリーン（スライド投影面）のマスクを推定し、
そこから四角形候補を1つ生成して detect 側のスコアリングに合流させる。

モデル契約（--ml-model に渡す ONNX）:
  - 入力 : NCHW float32, RGB, 0-1 正規化, サイズは (in_h, in_w)（既定 512x512）
  - 出力 : (1,1,H,W) または (1,H,W) の前景確率/ロジット。sigmoid 後 >0.5 を前景とみなす
モデルを渡さない場合、このモジュールは無効化され classical のみで動作する。
"""

from __future__ import annotations

from dataclasses import dataclass

import cv2
import numpy as np

from . import geometry as geo


@dataclass
class MLConfig:
    model_path: str
    input_size: tuple[int, int] = (512, 512)  # (w, h)
    threshold: float = 0.5
    providers: tuple[str, ...] = ("CPUExecutionProvider",)


class MLScreenDetector:
    """ONNX セグメンテーションモデルからスクリーン四隅候補を生成する。"""

    def __init__(self, cfg: MLConfig):
        import onnxruntime as ort  # 遅延 import（モデル未使用時に依存を強制しない）

        self.cfg = cfg
        self.session = ort.InferenceSession(cfg.model_path, providers=list(cfg.providers))
        self.input_name = self.session.get_inputs()[0].name

    def _infer_mask(self, image_bgr: np.ndarray) -> np.ndarray:
        h, w = image_bgr.shape[:2]
        in_w, in_h = self.cfg.input_size
        rgb = cv2.cvtColor(image_bgr, cv2.COLOR_BGR2RGB)
        resized = cv2.resize(rgb, (in_w, in_h), interpolation=cv2.INTER_AREA)
        blob = resized.astype(np.float32) / 255.0
        blob = np.transpose(blob, (2, 0, 1))[None, ...]  # NCHW
        out = self.session.run(None, {self.input_name: blob})[0]
        prob = np.squeeze(out).astype(np.float32)
        if prob.min() < 0 or prob.max() > 1:  # ロジットなら sigmoid
            prob = 1.0 / (1.0 + np.exp(-np.clip(prob, -30.0, 30.0)))
        mask = (prob > self.cfg.threshold).astype(np.uint8) * 255
        return cv2.resize(mask, (w, h), interpolation=cv2.INTER_NEAREST)

    def detect(self, image_bgr: np.ndarray) -> list[tuple[np.ndarray, str]]:
        """(quad_fullres, "ml") の候補リスト（0 or 1 件）を返す。"""
        mask = self._infer_mask(image_bgr)
        contours, _ = cv2.findContours(mask, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
        if not contours:
            return []
        c = max(contours, key=cv2.contourArea)
        peri = cv2.arcLength(c, True)
        for eps in (0.02, 0.04, 0.06, 0.08):
            approx = cv2.approxPolyDP(c, eps * peri, True)
            if len(approx) == 4:
                return [(geo.order_corners(approx.reshape(4, 2)), "ml")]
        # 四角形化に失敗したら最小外接矩形で代替
        box = cv2.boxPoints(cv2.minAreaRect(c))
        return [(geo.order_corners(box), "ml-minrect")]


def load_detector(model_path: str | None) -> MLScreenDetector | None:
    if not model_path:
        return None
    return MLScreenDetector(MLConfig(model_path=model_path))


class PersonSegmenter:
    """DeepLabV3(torchvision, Pascal VOC) による人物セグメンテーション。

    会場の観客・講演者など、スライドを遮る人物の領域マスクを返す。用途は2つ:
    (a) 検出前にエッジから人物領域を除去し、遮蔽シルエットで辺が引かれるのを防ぐ。
    (b) 台形補正後に切り出し内へ残った人物を inpaint で除去する。

    torch/torchvision は任意依存（`pip install 'slidewarp[ml]'`）。未導入なら
    分かりやすいエラーを出す。モデル重みは初回に自動ダウンロードされる。
    """

    PERSON_CLASS = 15  # Pascal VOC の person クラス

    def __init__(self, threshold: float = 0.5):
        try:
            import torch
            from torchvision.models.segmentation import (
                DeepLabV3_ResNet50_Weights,
                deeplabv3_resnet50,
            )
        except ImportError as e:  # pragma: no cover - 依存未導入時の案内
            raise RuntimeError(
                "人物セグメンテーションには torch/torchvision が必要です。"
                "`pip install 'slidewarp[ml]'` を実行してください。"
            ) from e
        self._torch = torch
        weights = DeepLabV3_ResNet50_Weights.DEFAULT
        self._model = deeplabv3_resnet50(weights=weights).eval()
        self._preprocess = weights.transforms()
        self.threshold = threshold

    def mask(self, image_bgr: np.ndarray) -> np.ndarray:
        """人物領域の 2値マスク（255=人物）を元画像サイズで返す。"""
        h, w = image_bgr.shape[:2]
        rgb = cv2.cvtColor(image_bgr, cv2.COLOR_BGR2RGB)
        tensor = self._torch.from_numpy(rgb).permute(2, 0, 1)
        batch = self._preprocess(tensor).unsqueeze(0)
        with self._torch.no_grad():
            out = self._model(batch)["out"][0]
        prob = self._torch.softmax(out, dim=0)[self.PERSON_CLASS].numpy()
        small = (prob > self.threshold).astype(np.uint8) * 255
        return cv2.resize(small, (w, h), interpolation=cv2.INTER_NEAREST)


def load_person_segmenter(enabled: bool) -> PersonSegmenter | None:
    return PersonSegmenter() if enabled else None
