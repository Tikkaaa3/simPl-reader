"""W14 independently recomputed idle negative controls on a copied real trace."""
import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
EVIDENCE = HERE / 'fixtures' / 'interaction-trace'
spec = importlib.util.spec_from_file_location('w14_analyzer', HERE / 'analyze-interaction.py')
analyzer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analyzer)


class IdleTraceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='w14-idle-test-')
        self.root = Path(self.temp.name)
        shutil.copyfile(EVIDENCE / 'manifest.json', self.root / 'manifest.json')
        self.run = self.root / '1000-scroll-800-1'
        self.run.mkdir()
        for name in ('outcome.json', 'app.csv'):
            shutil.copyfile(EVIDENCE / self.run.name / name, self.run / name)

    def tearDown(self):
        self.temp.cleanup()

    def tamper_idle(self, event):
        outcome = json.loads((self.run / 'outcome.json').read_text())
        timestamp = outcome['idle']['start'] + 1
        path = self.run / 'app.csv'
        lines = path.read_text().splitlines()
        event_line = f'{event},{timestamp},0.000,0'
        # Insertion is the only change to the run; update the footer count to keep
        # the trace syntactically valid, so the idle policy must reject it.
        for index in range(1, len(lines) - 1):
            if int(lines[index].split(',')[1]) > timestamp:
                lines.insert(index, event_line)
                break
        else:
            lines.insert(-1, event_line)
        footer = lines[-1].split(',')
        footer[3] = str(int(footer[3]) + 1)
        lines[-1] = ','.join(footer)
        path.write_text('\n'.join(lines) + '\n')

    def test_idle_redraw_is_rejected(self):
        self.tamper_idle('redraw')
        with self.assertRaisesRegex(ValueError, 'unexpected redraw during recorded idle interval'):
            analyzer.analyze(self.root)

    def test_idle_view_is_rejected(self):
        self.tamper_idle('view')
        with self.assertRaisesRegex(ValueError, 'unexpected view during recorded idle interval'):
            analyzer.analyze(self.root)


if __name__ == '__main__':
    unittest.main()
