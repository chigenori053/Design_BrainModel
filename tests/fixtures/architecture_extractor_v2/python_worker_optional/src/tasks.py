class TaskWorker:
    def run(self, payload: str) -> str:
        return f"done:{payload}"
