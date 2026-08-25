from abc import ABC, abstractmethod


class ProviderService(ABC):
    """Application extension point exposed by the Provider SDK demo."""

    @property
    @abstractmethod
    def implementation(self) -> str:
        """Return a short name identifying the concrete implementation."""

    @abstractmethod
    async def invoke(self, message: str) -> str:
        """Handle one demo invocation."""

