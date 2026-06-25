# retriever.py
import os
import requests
import chromadb
import re

chroma_path = os.path.expanduser("~/.config/aish/chromadb")
client = chromadb.PersistentClient(path=chroma_path)
collection = client.get_or_create_collection("codebase")

def get_embedding(text):
    response = requests.post(
        "http://localhost:11434/api/embeddings",
        json={"model": "nomic-embed-text", "prompt": text}
    )
    return response.json()["embedding"]

def retrieve(query, n_results=3):
    embedding = get_embedding(query)
    results = collection.query(
        query_embeddings=[embedding],
        n_results=n_results
    )
    chunks = results.get("documents", [[]])[0]
    files = [m["file"] for m in results.get("metadatas", [[]])[0]]
    return list(zip(files, chunks))
def retrieve_from_file(filename, query, n_results=5):
    embedding = get_embedding(query)

    results = collection.query(
        query_embeddings=[embedding],
        n_results=100
    )

    docs = results.get("documents", [[]])[0]
    metas = results.get("metadatas", [[]])[0]

    filtered = []

    for doc, meta in zip(docs, metas):
        filepath = meta.get("file", "")

        if filepath.endswith(filename):
            filtered.append((filepath, doc))

    return filtered[:n_results]


def extract_filename(query):
    m = re.search(
        r'\b[\w\-]+\.(c|h|py|js|ts|rs|md|txt)\b',
        query
    )

    return m.group(0) if m else None
   
