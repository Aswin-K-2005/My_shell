# indexer.py
import os
import requests
import chromadb
from chunker import chunk_file

# dirs to skip
SKIP_DIRS = {'venv', 'node_modules', '.git', '__pycache__', 'dist', 'build', 
             'site-packages', 'lib', 'app_flet', 'cv', 'env', '.env'}
# file types to index
INDEX_EXTS = {'.c', '.h', '.py', '.md', '.txt', '.js', '.ts'}

chroma_path = os.path.expanduser("~/.config/aish/chromadb")
client = chromadb.PersistentClient(path=chroma_path)
collection = client.get_or_create_collection("codebase")

def get_embedding(text):
    response = requests.post(
        "http://localhost:11434/api/embeddings",
        json={"model": "nomic-embed-text", "prompt": text}
    )
    return response.json()["embedding"]

def index_directory(dirpath):
    for root, dirs, files in os.walk(dirpath):
        # skip unwanted dirs
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        
        for filename in files:
            ext = os.path.splitext(filename)[1]
            if ext not in INDEX_EXTS:
                continue
            
            filepath = os.path.join(root, filename)
            print(f"Indexing: {filepath}")
            
            try:
                chunks = chunk_file(filepath)
                for i, chunk in enumerate(chunks):
                    if len(chunk.strip()) < 10:
                        continue
                    embedding = get_embedding(chunk)
                    doc_id = f"{filepath}::{i}"
                    collection.add(
                        embeddings=[embedding],
                        documents=[chunk],
                        ids=[doc_id],
                        metadatas=[{"file": filepath, "chunk": i}]
                    )
            except Exception as e:
                print(f"Error indexing {filepath}: {e}")

if __name__ == "__main__":
    home = os.path.expanduser("~")
    coding_dir = os.path.join(home, "coding/c")
    print(f"Indexing {coding_dir}...")
    index_directory(coding_dir)
    print("Done!")
