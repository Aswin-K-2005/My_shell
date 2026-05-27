
from fastapi import FastAPI
from pydantic import BaseModel
import requests
import json
from fastapi.middleware.cors import CORSMiddleware


class ChatRequest(BaseModel):
    message:str

app= FastAPI()
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

def callollama(message):
    prompt = f"You are an assistant for aish, an AI shell. Answer this: {message}"
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
    return result


@app.post("/chat")
def chat(request: ChatRequest):
    result=callollama(request.message)
    return {"response":result}

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
