"""Recompute W14 app callback durations from unmodified per-run CSV/JSON; not frames."""
import csv
import json
import math
import sys
from pathlib import Path


def nearest(values, percentile):
    if not values or not 0 < percentile <= 100:
        return None
    return sorted(values)[math.ceil(percentile * len(values) / 100) - 1]


def analyze(root):
    manifest = json.loads((root / 'manifest.json').read_text())
    assert manifest['schema'] == 'iced-w14/v1'
    for run in sorted(root.glob('*/outcome.json')):
        outcome = json.loads(run.read_text())
        lines = (run.parent / 'app.csv').read_text().splitlines()
        assert lines[0] == f"w14/v1,{manifest['qpc_frequency']},app,qpc"
        assert len(lines) >= 3 and len(lines) <= 200002
        assert outcome['status'] == 'VALID' and outcome['start'] < outcome['end']
        assert lines[-1].startswith('overhead,')
        footer = list(map(int, lines[-1].split(',')[1:]))
        assert len(footer) == 3 and footer[2] == len(lines) - 2
        assert footer[0] >= footer[1]
        prev = 0
        work = []
        callbacks = 0
        offsets, maxima, viewports = [], [], []
        idle = outcome.get('idle')
        if outcome['kind'] == 'scroll':
            assert idle is not None and outcome['end'] < idle['start'] < idle['end'] < outcome['ended']
        else:
            assert idle is None
        for row in csv.reader(lines[1:-1]):
            assert len(row) == 4
            event, ticks, value, duration = row
            ticks, duration = int(ticks), int(duration)
            assert event in ('wheel', 'scroll', 'maximum', 'viewport', 'width', 'resize', 'view', 'redraw')
            assert ticks > prev and duration <= ticks and math.isfinite(float(value))
            if idle is not None and idle['start'] <= ticks <= idle['end']:
                raise ValueError(f"{run.parent.name}: unexpected {event} during recorded idle interval")
            prev = ticks
            if outcome['start'] <= ticks <= outcome['end']:
                if event == 'view':
                    work.append(duration)
                    callbacks += 1
                elif event == 'scroll':
                    offsets.append(float(value))
                elif event == 'maximum':
                    maxima.append(float(value))
                elif event == 'viewport':
                    viewports.append(float(value))
        assert callbacks > 0
        assert outcome['close']['intentional'] and outcome['close']['exit_code'] == 0
        if outcome['kind'] == 'scroll':
            assert len(offsets) == len(maxima) and offsets[0] <= 120 and offsets[-1] <= 50
            assert any(high >= 50000 and abs(high - offset) <= 150
                       for offset, high in zip(offsets, maxima))
            assert outcome['capture'][0]['content_sha256'] == outcome['capture'][-1]['content_sha256']
        elif outcome['kind'] == 'resize':
            assert any(599.5 <= height <= 600.5 for height in viewports) and min(viewports) <= 500
            assert outcome['capture'][1]['pixels'] != outcome['capture'][2]['pixels']
        else:
            raise ValueError('unexpected scenario')
        frequency = manifest['qpc_frequency']
        print(outcome['id'], 'view_count=', callbacks,
              'view_callback_P50_ms=', round(1000 * nearest(work, 50) / frequency, 4),
              'view_callback_P95_ms=', round(1000 * nearest(work, 95) / frequency, 4),
              'all_trace_writer_ms=', round(1000 * footer[0] / frequency, 3),
              'peak_write_ms=', round(1000 * footer[1] / frequency, 3),
              'present_P50_P95=', 'UNAVAILABLE')


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('usage: python analyze-interaction.py <run-directory>')
    analyze(Path(sys.argv[1]))
