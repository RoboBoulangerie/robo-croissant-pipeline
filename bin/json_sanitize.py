import json
import argparse
import sys

def sanitize_json(file_path):
    try:
        with open(file_path, 'r') as f:
            data = json.load(f, strict=False)
        with open(file_path, 'w') as f:
            json.dump(data, f, indent=2)
            f.write('\n')
    except FileNotFoundError:
        print(f"Error: File '{file_path}' not found.", file=sys.stderr)
        sys.exit(1)
    except json.JSONDecodeError as e:
        print(f"Error: Failed to decode JSON from '{file_path}': {e}", file=sys.stderr)
        sys.exit(1)

def main():
    parser = argparse.ArgumentParser(description="Sanitize JSON file in-place.")
    parser.add_argument("file", help="Path to the JSON file to sanitize")
    args = parser.parse_args()

    sanitize_json(args.file)

if __name__ == "__main__":
    main()
