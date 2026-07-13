#!/bin/bash
# stop-all.sh — Stop all open-web-codex services
bash "$(dirname "$0")/start-all.sh" --stop
