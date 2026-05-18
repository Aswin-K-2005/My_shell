import requests
import json

cache = {}

while True:
    with open("/tmp/aish_in", "r") as f:
        error = f.read().strip()
    
    if error in cache:
        result = cache[error]
    else:
        prompt = f"Explain this shell error in one sentence and suggest a fix. Use the actual filename from the error: {error}"
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
    
    with open("/tmp/aish_out", "w") as f:
        f.write(result)
