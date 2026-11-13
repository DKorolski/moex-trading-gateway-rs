"""Synthetic offline verifier/collector regressions; never calls a broker."""
import copy
import datetime as dt
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"scripts"))
import finam_shadow_catalog_readonly as collect
import finam_shadow_catalog_verify as verify


def sample(model="USDRUBF"):
    row=dict(symbol=model+"@RTSX",id={"IMOEXF":"4631091","USDRUBF":"3447194","RIZ6":"5436517"}[model],
        ticker=model,mic="RTSX",isin="",type="FUTURES",name="RTS-12.26" if model=="RIZ6" else model,is_archived=False)
    lot={"IMOEXF":"10.0","USDRUBF":"1000.0","RIZ6":"1.0"}[model]
    asset={k:row[k] for k in ("id","ticker","mic","type","name")}
    asset.update(board="FUT",decimals={"IMOEXF":1,"USDRUBF":2,"RIZ6":0}[model],
        min_step={"IMOEXF":"5","USDRUBF":"1","RIZ6":"10"}[model],
        lot_size={"value":lot},quote_currency="USD" if model=="RIZ6" else "RUB",
        future_details={"contract_size":{"value":lot}})
    if model=="RIZ6":
        asset.update(expiration_date=dict(year=2026,month=12,day=17))
        asset["future_details"]["expiration_date"]="2026-12-16T21:00:00Z"
    params=dict(symbol=row["symbol"],trade_lot_size="1",is_tradable=True,tradeable=True,price_type="ANY")
    return row,asset,params


class CatalogTests(unittest.TestCase):
    def setUp(self):
        self.ri=json.loads((ROOT/"crates/finam-shadow-models/profiles/ri.json").read_bytes())

    def test_tick_exact_scaling(self):
        for args,expected in [(("5",1),"0.5"),(("1",2),"0.01"),(("10",0),"10"),
                              (("10",1),"1"),(("1000",3),"1"),(("1",9),"0.000000001")]:
            with self.subTest(args=args):
                self.assertEqual(verify.tick(*args),expected)

    def test_tick_rejects_ambiguous_or_invalid_values(self):
        for args in [("1e1",0),("1.0",0),("0",1),("-5",1),("+5",1),("5",True),
                     ("5",1.0),("5",-1),("5",10),("9"*19,0)]:
            with self.subTest(args=args),self.assertRaises(ValueError):
                verify.tick(*args)

    def test_reference_lot_not_used_as_contract_order_quantity(self):
        row,asset,params=sample()
        result=verify.bind("USDRUBF",row,asset,params,self.ri)
        self.assertEqual(result["reference_lot_size"],"1000.0")
        self.assertEqual(result["trade_lot_size"],"1")
        self.assertEqual(result["model_virtual_contract_qty"],1)
        params["trade_lot_size"]="1000"
        with self.assertRaisesRegex(ValueError,"ONE_CONTRACT_TRADE_LOT"):
            verify.bind("USDRUBF",row,asset,params,self.ri)

    def test_ri_expiry_utc_previous_date_is_same_moscow_date(self):
        row,asset,params=sample("RIZ6")
        result=verify.bind("RIZ6",row,asset,params,self.ri)
        self.assertEqual(result["expiration_date_msk"],"2026-12-17")
        asset["future_details"]["expiration_date"]="2026-12-16T20:59:59Z"
        with self.assertRaisesRegex(ValueError,"EXPIRY_DATE_DISAGREEMENT"):
            verify.bind("RIZ6",row,asset,params,self.ri)

    def test_metadata_identity_and_expiry_mismatches(self):
        cases=[("row","is_archived",True),("row","mic","MISX"),("row","symbol","RTS-12.26@RTSX"),
               ("row","ticker","RIH7"),("row","name","RTS-3.27"),("row","type","STOCK"),
               ("asset","id","wrong"),("asset","ticker","RIH7"),("asset","board","TQBR"),
               ("asset","quote_currency","UNKNOWN"),("params","symbol","RIH7@RTSX")]
        for section,key,value in cases:
            row,asset,params=sample("RIZ6")
            {"row":row,"asset":asset,"params":params}[section][key]=value
            with self.subTest(section=section,key=key),self.assertRaises(ValueError):
                verify.bind("RIZ6",row,asset,params,self.ri)

    def test_auth_readonly_account_permissions_and_actual_expiry(self):
        at=1_700_000_000_000
        details=dict(readonly=True,account_ids=["fixture-account"],
            expires_at=dt.datetime.fromtimestamp((at+900_000)/1000,dt.timezone.utc).isoformat(),
            md_permissions=[dict(mic="RTSX",delay_minutes=0)])
        self.assertEqual(collect.auth_check(details,"fixture-account",at),at+900_000)
        for key,value in [("readonly",False),("account_ids",["other"]),("md_permissions",[]),
                          ("expires_at",dt.datetime.fromtimestamp(at/1000,dt.timezone.utc).isoformat())]:
            bad=copy.deepcopy(details); bad[key]=value
            with self.subTest(key=key),self.assertRaises(collect.Hold):
                collect.auth_check(bad,"fixture-account",at)

    def test_account_membership_requires_list_of_strings(self):
        at=1_700_000_000_000
        details=dict(readonly=True,account_ids=["fixture-account"],
            expires_at=dt.datetime.fromtimestamp((at+900_000)/1000,dt.timezone.utc).isoformat(),
            md_permissions=[dict(mic="RTSX",delay_minutes=0)])
        collect.auth_check(details,"fixture-account",at)
        for bad in ("prefix-fixture-account-suffix", {"fixture-account":False}, [1], None):
            with self.subTest(value=bad),self.assertRaisesRegex(collect.Hold,"ACCOUNT_IDS_LIST"):
                collect.auth_check(dict(details,account_ids=bad),"fixture-account",at)
        with self.assertRaisesRegex(collect.Hold,"READONLY_ACCOUNT"):
            collect.auth_check(dict(details,account_ids=["prefix-fixture-account-suffix"]),"fixture-account",at)

    def test_shared_target_selection_rejects_symbol_and_id_conflicts(self):
        rows=[sample(m)[0] for m in ("IMOEXF","USDRUBF","RIZ6")]
        self.assertEqual(set(collect.selected_catalog_rows(rows)),{"IMOEXF","USDRUBF","RIZ6"})
        for case in ("symbol","id","cross_target","duplicate"):
            data=copy.deepcopy(rows)
            if case=="symbol":
                data.append(dict(data[1],ticker="OTHER",name="OTHER",id="9999999"))
            elif case=="id":
                data.append(dict(data[1],ticker="OTHER",symbol="OTHER@RTSX",name="OTHER"))
            elif case=="cross_target":
                data[2]["id"]=data[1]["id"]
            else:
                data.append(copy.deepcopy(data[1]))
            with self.subTest(case=case),self.assertRaises(collect.Hold):
                collect.selected_catalog_rows(data)

    def test_public_projection_guards_private_fields(self):
        collect.public_check({"id":"public-asset","future_details":{"contract_size":{"value":"1000"}}})
        for field in collect.PRIVATE_KEYS:
            with self.subTest(field=field),self.assertRaises(collect.Hold):
                collect.public_check({"nested":[{field:"fixture-private"}]})

    def test_env_is_parsed_without_shell_execution(self):
        with tempfile.TemporaryDirectory(prefix="catalog-env-test-") as root:
            path=Path(root)/"fixture.env"
            path.write_text("FINAM_SECRET_TOKEN='fixture-secret'\nFINAM_ACCOUNT_ID=fixture-account\n")
            self.assertEqual(collect.credentials(path),("fixture-secret","fixture-account"))
            path.write_text(f"FINAM_SECRET_TOKEN=$(touch {Path(root)/'marker'})\nFINAM_ACCOUNT_ID=fixture-account\n")
            with self.assertRaises(collect.Hold):
                collect.credentials(path)
            self.assertFalse((Path(root)/"marker").exists())

    def test_catalog_selector_excludes_other_month_venue_and_archive(self):
        rows=[sample(m)[0] for m in ("IMOEXF","USDRUBF","RIZ6")]
        other=copy.deepcopy(rows[2]); other.update(symbol="RIH7@RTSX",ticker="RIH7",name="RTS-3.27")
        archived=copy.deepcopy(rows[2]); archived["is_archived"]=True
        venue=copy.deepcopy(rows[2]); venue["mic"]="MISX"
        result=collect.candidates(rows+[other,archived,venue])
        self.assertTrue(all(len(v)==1 for v in result.values()))
        self.assertEqual(result["RIZ6"][0]["symbol"],"RIZ6@RTSX")

    def fixture(self,path):
        """Tiny declared synthetic HTTP trace for offline consistency tests."""
        at=1_700_000_000_000
        report=dict(status="COLLECTED_NOT_VALIDATED",auth=dict(readonly=True,
            selected_account_membership_verified=True,rtsx_zero_delay_verified=True,
            expires_at=dt.datetime.fromtimestamp((at+900_000)/1000,dt.timezone.utc).isoformat()),
            catalog_prefix_requests=[],catalog_page_count=1,catalog_rows=3,catalog_complete=True,
            activation=False,orders=False,websocket=False,bars=False,redis=False,
            raw_auth_retained=False,raw_account_retained=False,started_at_ms=at,finished_at_ms=at+10,
            instruments={},requests=[])
        for route in ("/v1/sessions","/v1/sessions/details"):
            report["requests"].append(dict(label="auth" if route=="/v1/sessions" else "details",
                method="POST",path=route,http_status=200,requested_at_ms=at,received_at_ms=at+1))
        body=verify.enc(dict(assets=[sample(m)[0] for m in ("IMOEXF","USDRUBF","RIZ6")],next_cursor=""))
        name="catalog-page-001.raw.json"; (path/name).write_bytes(body)
        report["requests"].append(dict(label="catalog_1",method="GET",path="/v1/assets/all",http_status=200,
            requested_at_ms=at+1,received_at_ms=at+2,transport_complete=True,original_raw_body_retained=True,
            file=name,response_sha256=verify.sha(body),bytes=len(body),http_content_length=len(body),
            public_catalog_query={"only_active":["true"]},query_keys=["only_active"]))
        for model in ("IMOEXF","USDRUBF","RIZ6"):
            row,asset,params=sample(model)
            report["instruments"][model]=dict(catalog_row=row,asset=asset,params=params,selected_account_matches=True)
            for label,value in (("asset",asset),("params",params)):
                name=model.lower()+"-"+label+".projection.json"; b=verify.enc(value); (path/name).write_bytes(b)
                report["requests"].append(dict(label=model+"_"+label,method="GET",
                    path="/v1/assets/"+model+"%40RTSX"+("/params" if label=="params" else ""),
                    query_keys=["account_id"],http_status=200,requested_at_ms=at+2,received_at_ms=at+3,
                    transport_complete=True,projection_file=name,projection_sha256=verify.sha(b),
                    original_raw_body_retained=False,scope="public_projection_not_original_raw",response_sha256="a"*64))
        (path/"collection.json").write_bytes(verify.enc(report))
        return report

    def test_full_offline_trace_positive(self):
        with tempfile.TemporaryDirectory(prefix="catalog-trace-test-") as directory:
            p=Path(directory); self.fixture(p)
            result=verify.verify(p)
            self.assertEqual(len(result["bindings"]),3)
            self.assertFalse(result["source_bars_admitted"])
            self.assertFalse(result["real_orders"])

    def test_full_offline_trace_rejects_auth_route_and_scope_tampering(self):
        for mutation in ("auth","post","cursor","closed","complete","expired"):
            with self.subTest(mutation=mutation),tempfile.TemporaryDirectory(prefix="catalog-negative-test-") as directory:
                p=Path(directory); r=self.fixture(p)
                if mutation=="auth": r["auth"]["readonly"]=False
                elif mutation=="post": r["requests"][3]["method"]="POST"
                elif mutation=="cursor": r["requests"][2]["public_catalog_query"]["cursor"]=["unexpected"]
                elif mutation=="closed": r["activation"]=True
                elif mutation=="complete": r["catalog_complete"]=False
                else: r["auth"]["expires_at"]=dt.datetime.fromtimestamp(r["finished_at_ms"]/1000,dt.timezone.utc).isoformat()
                (p/"collection.json").write_bytes(verify.enc(r))
                with self.assertRaises(ValueError):
                    verify.verify(p)

    def test_full_offline_trace_rejects_body_hash_or_projection_tamper(self):
        for name in ("catalog-page-001.raw.json","usdrubf-asset.projection.json"):
            with self.subTest(file=name),tempfile.TemporaryDirectory(prefix="catalog-body-test-") as directory:
                p=Path(directory); self.fixture(p); (p/name).write_bytes(b"{}")
                with self.assertRaises(ValueError):
                    verify.verify(p)


if __name__=="__main__":
    unittest.main()
