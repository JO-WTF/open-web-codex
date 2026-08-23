from __future__ import annotations

import unittest

from copilot_sdk.mock_responses import (
    ACCEPTANCE_FORK_TURNS,
    _has_child_task_envelope,
    _has_user_prompt,
    classify_request,
)


class MockResponsesRoutingTests(unittest.TestCase):
    def test_acceptance_spawn_keeps_one_turn_for_selected_capability_context(self) -> None:
        from copilot_sdk.mock_responses import _call

        item = _call(
            "spawn",
            "spawn_agent",
            {
                "task_name": "acceptance_worker",
                "message": "child task",
                "agent_type": "worker",
                "fork_turns": ACCEPTANCE_FORK_TURNS,
            },
            namespace="collaboration",
        )

        self.assertIn('"fork_turns":"1"', item["arguments"])

    def test_instructions_do_not_match_root_prompt(self) -> None:
        body = {
            "instructions": "Run the declared worker health check and return its verified result.",
            "input": [],
        }

        self.assertFalse(
            _has_user_prompt(
                body,
                "Run the declared worker health check and return its verified result.",
            )
        )

    def test_user_input_matches_root_prompt(self) -> None:
        prompt = "Run the declared worker health check and return its verified result."
        body = {
            "instructions": "unrelated",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": prompt}],
                }
            ],
        }

        self.assertTrue(_has_user_prompt(body, prompt))

    def test_classification_retains_only_structure_and_types(self) -> None:
        secret = "do-not-retain-this-request-body"
        body = {
            "instructions": f"worker agent {secret}",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "run acceptance"}],
                },
                {
                    "type": "function_call_output",
                    "call_id": "call-1",
                    "output": secret,
                },
            ],
        }

        classification = classify_request(
            body, root_prompt="run acceptance", child_task_name="acceptance_worker"
        )

        self.assertEqual(
            classification["toolOutputCallIds"], ["call-1"]
        )
        self.assertEqual(classification["userPrompt"], True)
        self.assertEqual(classification["child"], False)
        self.assertEqual(
            classification["toolOutputs"],
            [{"callId": "call-1", "outputType": "str"}],
        )
        self.assertNotIn(secret, repr(classification))

    def test_classification_extracts_only_allowed_json_field_types(self) -> None:
        body = {
            "input": [
                {
                    "type": "function_call_output",
                    "call_id": "spawn-call",
                    "output": '{"status":"failed","code":"bad_role","message":"Role missing","secret":"hide"}',
                }
            ]
        }

        classification = classify_request(
            body, root_prompt="prompt", child_task_name="acceptance_worker"
        )

        self.assertEqual(
            classification["toolOutputs"],
            [
                {
                    "callId": "spawn-call",
                    "outputType": "str",
                    "decodedType": "dict",
                    "fields": [
                        {"key": "status", "type": "str"},
                        {"key": "code", "type": "str"},
                    ],
                }
            ],
        )
        self.assertNotIn("secret", repr(classification))

    def test_classification_recurses_through_wrapped_json_output(self) -> None:
        body = {
            "input": [
                {
                    "type": "function_call_output",
                    "call_id": "spawn-call",
                    "output": '{"output":"{\\"status\\":\\"completed\\",\\"agent_id\\":\\"agent-1\\",\\"content\\":[{\\"type\\":\\"text\\",\\"text\\":\\"spawned\\"}]}"}',
                }
            ]
        }

        classification = classify_request(
            body, root_prompt="prompt", child_task_name="acceptance_worker"
        )

        self.assertEqual(
            classification["toolOutputs"][0]["fields"],
            [
                {"key": "status", "type": "str"},
                {"key": "agent_id", "type": "str"},
            ],
        )

    def test_classification_records_only_list_item_types(self) -> None:
        body = {
            "input": [
                {
                    "type": "function_call_output",
                    "call_id": "spawn-call",
                    "output": [
                        {"type": "text", "text": "spawn complete"},
                        {"type": "meta", "content": "bounded detail"},
                    ],
                }
            ]
        }

        classification = classify_request(
            body, root_prompt="prompt", child_task_name="acceptance_worker"
        )

        self.assertEqual(
            classification["toolOutputs"][0],
            {
                "callId": "spawn-call",
                "outputType": "list",
                "itemTypes": ["text", "meta"],
            },
        )

    def test_root_user_prompt_is_not_child_task_envelope(self) -> None:
        body = {
            "input": [
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "root task"}],
                }
            ]
        }

        self.assertFalse(_has_child_task_envelope(body, "acceptance_worker"))

    def test_native_child_task_envelope_is_exact_and_encrypted(self) -> None:
        body = {
            "input": [
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "root task"}],
                },
                {
                    "type": "function_call_output",
                    "call_id": "copilot-root-spawn",
                    "output": "spawned",
                },
                {
                    "type": "agent_message",
                    "content": [
                        {
                            "type": "input_text",
                            "text": "Message Type: NEW_TASK\nTask name: /root/acceptance_worker\nSender: /root\nPayload:\n",
                        },
                        {"type": "encrypted_content", "data": "opaque"},
                    ],
                },
            ]
        }

        self.assertTrue(_has_child_task_envelope(body, "acceptance_worker"))


if __name__ == "__main__":
    unittest.main()
