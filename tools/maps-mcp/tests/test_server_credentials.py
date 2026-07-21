from __future__ import annotations

import asyncio
import tempfile
import unittest
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request
from urllib.request import urlopen

import maps_mcp.server as server
from maps_mcp.clients import GoogleMapsClient
from maps_mcp.credentials import WorkspaceCredentialStore


class AcceptedResult:
    action = "accept"


class FakeSession:
    def __init__(self) -> None:
        self.completed: list[str] = []

    async def send_elicit_complete(self, elicitation_id: str) -> None:
        self.completed.append(elicitation_id)


class FakeContext:
    def __init__(self) -> None:
        self.session = FakeSession()
        self.messages: list[str] = []

    async def elicit_url(self, *, message: str, url: str, elicitation_id: str):
        self.messages.append(message)
        body = urlencode({"api_key": "workspace-secret", "remember": "yes"}).encode()
        request = Request(url, data=body, method="POST")
        await asyncio.to_thread(lambda: urlopen(request).read())
        return AcceptedResult()

    async def info(self, message: str) -> None:
        self.messages.append(message)


class ServerCredentialTests(unittest.IsolatedAsyncioTestCase):
    async def test_missing_key_is_collected_and_remembered_per_workspace(self) -> None:
        original_store = server._credential_store
        with tempfile.TemporaryDirectory() as temp_dir:
            store = WorkspaceCredentialStore(Path(temp_dir))
            server._credential_store = store
            context = FakeContext()
            try:
                client = await server._client("google", context)
            finally:
                server._credential_store = original_store

            self.assertIsInstance(client, GoogleMapsClient)
            self.assertEqual(store.get_api_key("google"), "workspace-secret")
            self.assertEqual(len(context.session.completed), 1)
            self.assertTrue(any("Stored google API key" in message for message in context.messages))


if __name__ == "__main__":
    unittest.main()
