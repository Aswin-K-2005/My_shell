import requests
import json
import os

cache = {}

def read_memory():
    path = os.path.expanduser("~/.config/aish/memory.json")
    try:
        with open(path, "r") as f:
            return json.load(f)
    except:
        return {}

while True:
    try:
        with open("/tmp/aish_in", "r") as f:
            message = f.read().strip()
    
        if message.startswith("nlp:"):
            query = message[4:]
            memory = read_memory()
            context = f"User's current directory: {memory.get('last_dir', 'unknown')}. Last command: {memory.get('last_command', 'unknown')}."
            prompt = f"{context} Convert this to a shell command. Examples: 'show python files' -> 'find . -name \"*.py\"', 'count lines in file' -> 'wc -l filename'. Return ONLY the command: {query}"  
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
    except KeyboardInterrupt:
            continue
    except Exception as e:
            continue
