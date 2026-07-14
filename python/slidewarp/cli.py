"""slidewarp CLI エントリポイント。"""

from __future__ import annotations

import argparse
import os
import sys
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

from . import __version__, io_utils
from . import report as report_mod
from .ml import load_detector, load_person_segmenter
from .pipeline import ProcessConfig, ProcessResult, process_image

# 各ワーカープロセスで一度だけ初期化する状態
_WORKER_CFG: ProcessConfig | None = None
_WORKER_DETECTOR = None
_WORKER_PERSON = None


def _init_worker(cfg: ProcessConfig, model_path: str | None, remove_people: bool) -> None:
    global _WORKER_CFG, _WORKER_DETECTOR, _WORKER_PERSON
    _WORKER_CFG = cfg
    _WORKER_DETECTOR = load_detector(model_path)
    _WORKER_PERSON = load_person_segmenter(remove_people)


def _worker(src_str: str) -> ProcessResult:
    return process_image(Path(src_str), _WORKER_CFG, _WORKER_DETECTOR, _WORKER_PERSON)


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="slidewarp",
        description="学会撮影スライド写真を一括で検出・台形補正・シャープ化する。",
    )
    p.add_argument("inputs", nargs="+", help="写真ファイル または フォルダ（再帰探索）")
    p.add_argument("-o", "--out-dir", required=True, type=Path, help="出力ディレクトリ")
    p.add_argument("--exposure", action="store_true", help="露出/コントラストの自動補正を行う")
    p.add_argument("--color", action="store_true", help="ホワイトバランス補正を行う")
    p.add_argument("--sharpen", type=float, default=1.0, help="シャープ化の強さ（0で無効）")
    p.add_argument("--ml-model", default=None,
                   help="スクリーン検出用 ONNX セグメンテーションモデル（省略時 classical のみ）")
    p.add_argument("--remove-people", action="store_true",
                   help="人物セグメンテーション(DeepLabV3)で遮蔽者を検出から除外し、"
                        "切り出し内に残った人物を inpaint 除去する（要 'slidewarp[ml]'、低速）")
    p.add_argument("--min-confidence", type=float, default=0.35,
                   help="この信頼度未満は誤補正回避のため出力しない")
    p.add_argument("--on-low-confidence", choices=["skip", "copy"], default="skip",
                   help="低信頼時の挙動: skip=出さない / copy=原本を _review へ退避")
    p.add_argument("--max-long-side", type=int, default=1600, help="出力画像の長辺(px)上限")
    p.add_argument("--margin", type=float, default=0.03,
                   help="検出矩形を各辺この比率だけ外へ広げて切り出す（周辺マージン。0で無効）")
    p.add_argument("--ext", default=".jpg", help="出力拡張子（例 .jpg/.png）")
    p.add_argument("--jpeg-quality", type=int, default=95, help="JPEG 品質(1-100)")
    p.add_argument("-j", "--jobs", type=int, default=0,
                   help="並列数（0で自動: CPU数）。1で逐次実行")
    p.add_argument("--no-report", action="store_true",
                   help="評価用 report.html を生成しない（既定は生成する）")
    p.add_argument("-V", "--version", action="version", version=f"slidewarp {__version__}")
    return p


def _log(res: ProcessResult) -> None:
    tag = {"ok": "OK  ", "low_confidence": "LOW ", "no_detection": "NONE",
           "error": "ERR "}.get(res.status, "?   ")
    detail = f"conf={res.confidence:.2f} {res.method}".strip()
    extra = f" -> {res.out_path.name}" if res.out_path else ""
    msg = f" ({res.message})" if res.message else ""
    print(f"[{tag}] {res.src.name}  {detail}{extra}{msg}", flush=True)


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)

    files = io_utils.collect_inputs([str(x) for x in args.inputs])
    if not files:
        print("入力に処理可能な画像が見つかりませんでした。", file=sys.stderr)
        return 2

    cfg = ProcessConfig(
        out_dir=args.out_dir,
        min_confidence=args.min_confidence,
        on_low_confidence=args.on_low_confidence,
        sharpen=args.sharpen,
        exposure=args.exposure,
        color=args.color,
        max_long_side=args.max_long_side,
        margin=args.margin,
        out_ext=args.ext if args.ext.startswith(".") else f".{args.ext}",
        jpeg_quality=args.jpeg_quality,
    )

    if args.jobs > 0:
        jobs = args.jobs
    elif args.remove_people:
        # DeepLabV3 はワーカーごとにモデルを読み込みメモリ・CPUを食うので控えめに
        jobs = min(4, os.cpu_count() or 1)
    else:
        jobs = os.cpu_count() or 1
    feats = []
    if args.ml_model:
        feats.append("screen-ML")
    if args.remove_people:
        feats.append("remove-people")
    print(f"対象 {len(files)} 枚 / 並列 {jobs} / 機能={','.join(feats) or 'なし'}")

    results: list[ProcessResult] = []
    if jobs == 1:
        _init_worker(cfg, args.ml_model, args.remove_people)
        for f in files:
            res = _worker(str(f))
            _log(res)
            results.append(res)
    else:
        with ProcessPoolExecutor(
            max_workers=jobs, initializer=_init_worker,
            initargs=(cfg, args.ml_model, args.remove_people),
        ) as ex:
            for res in ex.map(_worker, [str(f) for f in files]):
                _log(res)
                results.append(res)

    counts: dict[str, int] = {}
    for r in results:
        counts[r.status] = counts.get(r.status, 0) + 1
    print("---")
    print("集計: " + ", ".join(f"{k}={v}" for k, v in sorted(counts.items())))

    if not args.no_report:
        report_path = report_mod.write_report(
            results, cfg, cfg.out_dir,
            opts={"exposure": args.exposure, "color": args.color,
                  "sharpen": args.sharpen, "ml": bool(args.ml_model)},
        )
        print(f"レビュー: {report_path}")
    # 1枚も成功しなければ非0を返す
    return 0 if counts.get("ok", 0) > 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
