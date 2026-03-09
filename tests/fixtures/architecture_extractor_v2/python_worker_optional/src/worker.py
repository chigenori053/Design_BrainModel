from tasks import TaskWorker


class EventHandler:
    def dispatch(self, payload: str) -> str:
        return TaskWorker().run(payload)
