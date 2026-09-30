#!/usr/bin/env python3
"""Public-template regressions; never contact FINAM, Redis or an operational host."""
import copy
import unittest

import stage8b_p1f_o2_artifact_check as check


class ProfileBindingTests(unittest.TestCase):
    def setUp(self):
        root = check.ROOT / "docs/stage-8"
        self.profile = check.load_json(root / "stage8b-p1e-runtime-profile-v1.json")
        self.source = check.load_json(root / "stage8b-p1f-o2-source-template.json")
        self.config = check.load_json(root / "stage8b-p1f-o2-supervisor-template.json")

    def test_accepted_templates(self):
        check.validate_profile_binding(self.profile, self.source, self.config)

    def test_rejects_twelve_binding_mutations(self):
        # Positive control makes rejection meaningful: no pre-existing error.
        check.validate_profile_binding(self.profile, self.source, self.config)
        cases = [
            ("profile-id", "profile", ("profile_id",), "imoexf-hybrid-high180-paper-v1"),
            ("mr-enabled", "profile", ("semantic_config", "live_mr_entries_enabled"), True),
            ("model-start", "profile", ("semantic_config", "model_session_start_time"), "09:00:00"),
            ("source-hash", "source", ("runtime_profile_sha256",), "dd5a211e708db0d40175d19ed1eeb51db26497d344a553b41d7afbfdddde0ef6"),
            ("source-version", "source", ("schema_version",), 1),
            ("source-domain", "source", ("domain",), "moex.stage8b.p1e.first-boot-source-bundle.v1"),
            ("supervisor-version", "config", ("schema_version",), 2),
            ("supervisor-id", "config", ("runtime_profile_id",), "imoexf-hybrid-high180-paper-v1"),
            ("supervisor-hash", "config", ("runtime_profile_sha256",), "0" * 64),
            ("runtime-fingerprint", "config", ("bootstrap", "runtime_config_fingerprint_sha256"), "60793364de8e60744235b7270a3324d0b9868fc93290c480c81a5d88f6ac6b24"),
            ("source-sentinel", "config", ("first_boot_source_bundle_sha256",), "0" * 64),
            ("account-alias", "config", ("bootstrap", "account_id"), "foreign"),
        ]
        for name, target, path, value in cases:
            with self.subTest(name=name):
                documents = copy.deepcopy({"profile": self.profile, "source": self.source, "config": self.config})
                node = documents[target]
                for key in path[:-1]:
                    node = node[key]
                self.assertNotEqual(node[path[-1]], value)
                node[path[-1]] = value
                with self.assertRaises(check.ArtifactError):
                    check.validate_profile_binding(**documents)


if __name__ == "__main__":
    unittest.main()
