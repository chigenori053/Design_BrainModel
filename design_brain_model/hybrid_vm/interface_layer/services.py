from design_brain_model.brain_model.api import DesignBrainModel

class InterfaceServices:
    """
    Gateway for external services like BrainModel and UI.
    """
    def __init__(self):
        self._brain = DesignBrainModel()

    @property
    def brain(self) -> DesignBrainModel:
        return self._brain
