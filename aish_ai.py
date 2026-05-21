import requests
import json

cache = {}

while True:
    with open("/tmp/aish_in", "r") as f:
        message = f.read().strip()
    
    if message.startswith("nlp:"):
        query = message[4:]
        prompt = f"Convert this to a shell command. Examples: 'show python files' -> 'find . -name \"*.py\"', 'count lines in file' -> 'wc -l filename'. Return ONLY the command: {query}"  
        response = requests.post(
            "http://localhost:11434/api/generate",
            json={
                "model": "qwen3.5:9b",
                "prompt": prompt,
                "stream": False,
                "think": False
            }
        )
        result = response.json()["response"].strip()
    
    elif message.startswith("error:"):
        error = message[6:]
        if error in cache:
            result = cache[error]
        else:
            prompt = f"In one sentence explain why this shell command failed and how to fix it: {error}"
            response = requests.post(
                "http://localhost:11434/api/generate",
                json={
                    "model": "qwen3.5:9b",
                    "prompt": prompt,
                    "stream": False,
                    "think": False
                }
            )
            result = response.json()["response"]
            cache[error] = result
    
    else:
        result = "unknown request type"
    
    with open("/tmp/aish_out", "w") as f:
        f.write(result)
