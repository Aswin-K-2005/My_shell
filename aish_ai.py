import requests
import json
import sys
import os
import re

MODEL = "qwen3.5:9b"

def ollama(prompt, temperature=0.0, think=False):
    response = requests.post(
        "http://localhost:11434/api/generate",
        json={
            "model": MODEL,
            "prompt": prompt,
            "stream": False,
            "think": think,
            "options": {"temperature": temperature}
        }
    )
    return response.json()["response"].strip()

while True:
    try:
        with open("/tmp/aish_in", "r") as f:
            message = f.read().strip()

        if message.startswith("nlp:"):
            query = message[4:]
            prompt = f"""You are a shell command translator. Convert to a single shell command.

Rules:
- Return ONLY the command, nothing else
- No explanation, no markdown, no backticks
- Always exclude: venv, node_modules, .git, __pycache__, .pyc, dist, build
- For finding files use: find . -name "*.py" -not -path "*/venv/*" -not -path "*/__pycache__/*"
- Current directory is already provided in query for context only, do NOT cd to it

Query: {query}"""            
            result = ollama(prompt, temperature=0)

        elif message.startswith("error:"):
            error = message[6:]
            prompt = f"""You are a shell error explainer. Explain this error in ONE sentence and give the fix.

Rules:
- One sentence only
- Plain English, no placeholders like <path> or <FILE>
- Be specific about what went wrong
- Give exact fix command if possible

Error: {error}"""
            result = ollama(prompt, temperature=0)

        elif message.startswith("chat:"):
            query = message[5:]
            prompt = f"""You are Aish, an AI assistant built into a Unix shell.
You help developers with coding, Linux, debugging, and shell tasks.
You are concise, technical, and helpful.
You know the user is a developer working in a terminal.

User: {query}
Aish:"""
            result = ollama(prompt, temperature=0.7, think=False)

        else:
            result = "unknown request type"

        with open("/tmp/aish_out", "w") as f:
            f.write(result)

    except KeyboardInterrupt:
        continue
    except Exception as e:
        continue    
