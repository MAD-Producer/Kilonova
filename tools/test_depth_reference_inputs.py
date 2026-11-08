"""Independent timestamp association and malformed-input controls."""
import unittest

import depth_reference_inputs as inputs


def frame(ns, kind):
    return inputs.Frame(ns, f'{kind}/{ns}.png')


class ReferenceInputs(unittest.TestCase):
    def test_epoch_nanoseconds_preserve_submicrosecond_order(self):
        rows = inputs.parse('# source\n1305031452.000000001 rgb/1305031452.000000001.png\n'
                            '1305031452.000000002 rgb/1305031452.000000002.png\n', 'rgb')
        self.assertEqual(rows[0].timestamp_ns, 1305031452000000001)
        self.assertEqual(rows[1].timestamp_ns - rows[0].timestamp_ns, 1)

    def test_global_nearest_matching_consumes_each_frame_once(self):
        pairs = inputs.associate([frame(v, 'rgb') for v in (0, 30_000_000, 60_000_000)],
                                 [frame(v, 'depth') for v in (19_000_000, 50_000_000)], count=2)
        self.assertEqual([(p.rgb.timestamp_ns, p.depth.timestamp_ns) for p in pairs],
                         [(30_000_000, 19_000_000), (60_000_000, 50_000_000)])
        self.assertEqual(pairs[0].signed_difference_ns, -11_000_000)

    def test_equal_difference_prefers_earlier_rgb_then_depth(self):
        pairs = inputs.associate([frame(v, 'rgb') for v in (0, 20_000_000)],
                                 [frame(10_000_000, 'depth')], count=1)
        self.assertEqual(pairs[0].rgb.timestamp_ns, 0)
        pair = inputs.associate([frame(10_000_000, 'rgb')],
                                [frame(v, 'depth') for v in (0, 20_000_000)], count=1)[0]
        self.assertEqual(pair.depth.timestamp_ns, 0)

    def test_limit_is_inclusive_and_not_relaxed_for_missing_pairs(self):
        one = [frame(0, 'rgb')]
        self.assertEqual(len(inputs.associate(one, [frame(20_000_000, 'depth')], count=1)), 1)
        with self.assertRaisesRegex(ValueError, 'insufficient'):
            inputs.associate(one, [frame(20_000_001, 'depth')], count=1)

    def test_only_first32_sorted_pairs_are_selected(self):
        rgb = [frame(v * 100_000_000, 'rgb') for v in range(40)]
        depth = [frame(v.timestamp_ns+1, 'depth') for v in rgb]
        pairs = inputs.associate(rgb, depth)
        self.assertEqual(len(pairs), 32)
        self.assertEqual(pairs[0].rgb.timestamp_ns, 0)
        self.assertEqual(pairs[-1].rgb.timestamp_ns, 3_100_000_000)

    def test_paths_duplicates_nonfinite_and_unordered_rows_refuse(self):
        for text in ('NaN rgb/NaN.png', '1 rgb/../1.png', '1 depth/1.png',
                     '1 rgb/2.png', '1 rgb/1.png\n1 rgb/1.png',
                     '2 rgb/2.png\n1 rgb/1.png', '1.0000000001 rgb/1.0000000001.png'):
            with self.subTest(text=text):
                with self.assertRaises(ValueError):
                    inputs.parse(text, 'rgb')


if __name__ == '__main__':
    unittest.main()
