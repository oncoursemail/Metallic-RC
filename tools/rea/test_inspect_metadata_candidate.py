import importlib.util
import pathlib
import struct
import unittest

p=pathlib.Path(__file__).with_name("inspect_metadata_candidate.py")
spec=importlib.util.spec_from_file_location("metadata_inspect",p)
m=importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class ValidationTests(unittest.TestCase):
    def tile_fixture(self):
        vals=[]
        for i in range(31):
            vals.extend([256+i*64,64])
        vals.extend([0xFAFAFAFA,0xABABABAB])
        # A deterministic non-paired permutation exercises the actual format.
        vals=vals[::2]+vals[1::2]
        return struct.pack("<64I",*vals)+bytes(31*64)

    def test_permuted_individual_ints(self):
        b=self.tile_fixture()
        r=m.solve_header(b,len(b))
        self.assertTrue(r["searchComplete"])
        self.assertEqual(len(r["solutions"]),1)
        self.assertEqual([r["size"] for r in r["solutions"][0]["regions"]],[64]*31)

    def test_wrong_length_is_not_accepted(self):
        b=self.tile_fixture()+b"\0"
        self.assertEqual(m.solve_header(b,len(b))["solutions"],[])

    def test_uniform_R_has_no_metadata(self):
        r=m.inspect(b"R"*1024,1024)
        self.assertEqual(r["status"],"uniform_R_placeholder")
        self.assertFalse(r["decoded"])
        self.assertNotIn("headerSearch",r)

    def test_search_limit_does_not_claim_complete(self):
        r=m.solve_header(self.tile_fixture(),2240,max_states=2)
        self.assertFalse(r["searchComplete"])
        self.assertEqual(r["stoppedReason"],"state_limit")

    def test_literal_data_range_validation(self):
        b=bytearray(300)
        struct.pack_into("<4I",b,256,3,0,2,3)
        regions=[{"index":0,"offset":256,"size":16},
                 {"index":1,"offset":272,"size":5}]
        self.assertEqual(len(m.literal_candidates(b,regions)),1)
        struct.pack_into("<I",b,268,4)
        self.assertEqual(m.literal_candidates(b,regions),[])

    def test_string_bounds_and_unicode(self):
        b="éclair\0".encode()+b"x"*20
        blob={"offset":0,"size":len(b)}
        self.assertEqual(m.str_at(b,blob,0),"éclair")
        self.assertIsNone(m.str_at(b,blob,len(b)))
        self.assertIsNone(m.str_at(b,blob,8))

    def test_partial_capture_refused(self):
        with self.assertRaises(ValueError):
            m.solve_header(bytes(256),52403096)


if __name__=="__main__":
    unittest.main(verbosity=2)
