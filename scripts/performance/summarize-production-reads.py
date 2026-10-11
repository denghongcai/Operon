#!/usr/bin/env python3
"""Validate and summarize retained matched production read matrices."""
import argparse
import json
import statistics
from pathlib import Path


def collect(path):
    summary = json.loads((path / 'summary.json').read_text())
    environment = json.loads((path / 'environment.json').read_text())
    trials = {}
    hashes = None
    for side in ('baseline', 'candidate'):
        trials[side] = []
        for index in range(3):
            trial = path / f'{side}-{index}'
            reads = json.loads((trial / 'reads.json').read_text())
            evidence = json.loads((trial / 'evidence.json').read_text())
            assert len(reads) == 12, (trial, len(reads))
            assert {(r['mode'], r['readers']) for r in reads} == {
                (mode, readers) for mode in ('same', 'distinct', 'random')
                for readers in (1, 4, 8, 16)
            }, trial
            for row in reads:
                assert row['operations'] > 0 and row['seconds'] > 0, trial
                expected = row['operations'] / row['seconds']
                assert abs(expected - row['ops_per_second']) < expected * 1e-9, trial
                key = f"{row['mode']}/{row['readers']}"
                assert row['ops_per_second'] in summary[side][key]['trials'], trial
            if hashes is None:
                hashes = evidence['binary_sha256']
            assert evidence['binary_sha256'] == hashes, trial
            trials[side].append({'reads': reads, 'evidence': evidence})
        for key, entry in summary[side].items():
            values = [row['ops_per_second'] for trial in trials[side]
                      for row in trial['reads']
                      if f"{row['mode']}/{row['readers']}" == key]
            assert sorted(values) == sorted(entry['trials']), (path, side, key)
            assert statistics.median(values) == entry['median'], (path, side, key)
    comparisons = {
        key: {'sync_ops_s': summary['baseline'][key]['median'],
              'async_ops_s': summary['candidate'][key]['median'],
              'async_over_sync': summary['candidate'][key]['median'] /
                                 summary['baseline'][key]['median']}
        for key in summary['baseline']
    }
    return {'path': str(path), 'cases': 72, 'environment': environment,
            'binary_sha256': hashes, 'summary': summary,
            'comparisons': comparisons, 'trials': trials}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('matrices', nargs='+', type=Path)
    parser.add_argument('--output', type=Path,
                        help='Create a new evidence file; refuses to overwrite')
    args = parser.parse_args()
    result = {path.name: collect(path) for path in args.matrices}
    assert len({json.dumps(row['binary_sha256'], sort_keys=True)
                for row in result.values()}) == 1, 'different matrix binaries'
    serialized = json.dumps(result, indent=2) + '\n'
    if args.output:
        with args.output.open('x') as output:
            output.write(serialized)
        print(f'Validated {len(result) * 72} cases: {args.output}')
    else:
        print(serialized, end='')


if __name__ == '__main__':
    main()
