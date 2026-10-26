import unittest
from isolation import interval_busy_percent, conflicts

class AdmissionTests(unittest.TestCase):
    def test_idle(self):
        self.assertEqual(interval_busy_percent([10,20,30,40], [10,20,1030,40], 3), 0)
    def test_one_full_cpu_on_three(self):
        self.assertEqual(interval_busy_percent([0]*4, [100,0,200,0], 3), 100)
    def test_counter_wrap(self):
        self.assertEqual(interval_busy_percent([2**32-10,0,0,0], [0,0,20,0], 3), 100)
    def test_missing_clock(self):
        with self.assertRaises(ValueError): interval_busy_percent([0]*4, [0]*4, 3)
    def test_conflicting_process_even_when_idle(self):
        self.assertTrue(conflicts({'processes':[{'executable':'/bin/cargo','cpu_percent':0}]}))

if __name__ == '__main__': unittest.main()
