import os
import sys
import subprocess
import requests
import chromadb
from chunker import chunk_file

SKIP_DIRS = {
    'venv', 'node_modules', '.git', '__pycache__',
    'dist', 'build', 'site-packages', 'lib',
    'app_flet', 'cv', 'env', '.env'
}

INDEX_EXTS = {
    '.c', '.h', '.py', '.md',
    '.txt', '.js', '.ts', '.rs'
}

chroma_path = os.path.expanduser("~/.config/aish/chromadb")
client = chromadb.PersistentClient(path=chroma_path)
collection = client.get_or_create_collection("codebase")


def get_project_root(start_dir=None):
    if start_dir is None:
        start_dir = os.getcwd()

    try:
        root = subprocess.check_output(
            ["git", "-C", start_dir, "rev-parse", "--show-toplevel"],
            text=True
        ).strip()
        return root
    except Exception:
        return os.path.abspath(start_dir)


def get_embedding(text):
    response = requests.post(
        "http://localhost:11434/api/embeddings",
        json={
            "model": "nomic-embed-text",
            "prompt": text
        }
    )

    response.raise_for_status()
    return response.json()["embedding"]


def index_directory(project_root):
    print(f"\nIndexing project: {project_root}\n")

    indexed = 0

    for root, dirs, files in os.walk(project_root):
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

                    collection.upsert(
                        ids=[doc_id],
                        embeddings=[embedding],
                        documents=[chunk],
                        metadatas=[{
                            "file": filepath,
                            "chunk": i,
                            "project": project_root
                        }]
                    )

                    indexed += 1

            except Exception as e:
                print(f"Error indexing {filepath}: {e}")

    print(f"\nDone. Indexed {indexed} chunks.\n")


if __name__ == "__main__":

    if len(sys.argv) > 1:
        target_dir = os.path.abspath(sys.argv[1])
    else:
        target_dir = os.getcwd()

    project_root = get_project_root(target_dir)

    print(f"Detected project root: {project_root}")

    index_directory(project_root)   
    print("Done!")
