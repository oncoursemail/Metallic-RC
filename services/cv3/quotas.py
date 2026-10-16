"""
CV3 Quotas System
Implements per-tick quotas for execution time, memory usage, and operation limits.
"""

from enum import Enum
from dataclasses import dataclass
from typing import Dict, Optional
import time


class QuotaType(Enum):
    """Types of quotas that can be enforced."""
    TICK_TIME_MS = "tick_time_ms"      # Max milliseconds per tick
    MEMORY_BYTES = "memory_bytes"      # Max memory allocation in bytes
    OPERATION_COUNT = "operation_count" # Max operations per tick
    VARIABLE_COUNT = "variable_count"   # Max number of variables
    NODE_EXECUTIONS = "node_executions" # Max node executions per tick


@dataclass
class CV3Quota:
    """A single quota limit."""
    quota_type: QuotaType
    limit: float  # The maximum allowed value
    current_usage: float = 0.0
    violated: bool = False
    
    def reset(self):
        """Reset current usage for a new tick."""
        self.current_usage = 0.0
        self.violated = False
    
    def use(self, amount: float) -> bool:
        """Use some amount of the quota and return True if within limits."""
        self.current_usage += amount
        if self.current_usage > self.limit:
            self.violated = True
            return False
        return True
    
    def get_remaining(self) -> float:
        """Get remaining quota."""
        return max(0.0, self.limit - self.current_usage)
    
    def get_usage_percent(self) -> float:
        """Get usage as percentage of limit."""
        if self.limit == 0:
            return 0.0
        return min(100.0, (self.current_usage / self.limit) * 100.0)


@dataclass
class CV3Quotas:
    """Collection of quotas for CV3 graph execution."""
    quotas: Dict[QuotaType, CV3Quota] = None
    
    def __post_init__(self):
        if self.quotas is None:
            self.quotas = {}
            # Set default quotas
            self.quotas[QuotaType.TICK_TIME_MS] = CV3Quota(QuotaType.TICK_TIME_MS, 16.0)  # 16ms = ~60 FPS
            self.quotas[QuotaType.MEMORY_BYTES] = CV3Quota(QuotaType.MEMORY_BYTES, 1024 * 1024)  # 1MB
            self.quotas[QuotaType.OPERATION_COUNT] = CV3Quota(QuotaType.OPERATION_COUNT, 1000)
            self.quotas[QuotaType.VARIABLE_COUNT] = CV3Quota(QuotaType.VARIABLE_COUNT, 100)
            self.quotas[QuotaType.NODE_EXECUTIONS] = CV3Quota(QuotaType.NODE_EXECUTIONS, 50)
    
    def set_quota(self, quota_type: QuotaType, limit: float):
        """Set or update a quota limit."""
        if quota_type not in self.quotas:
            self.quotas[quota_type] = CV3Quota(quota_type, limit)
        else:
            self.quotas[quota_type].limit = limit
    
    def get_quota(self, quota_type: QuotaType) -> Optional[CV3Quota]:
        """Get a quota by type."""
        return self.quotas.get(quota_type)
    
    def reset_all(self):
        """Reset all quotas for a new tick."""
        for quota in self.quotas.values():
            quota.reset()
    
    def use_quota(self, quota_type: QuotaType, amount: float) -> bool:
        """Use some amount of a quota and return True if within limits."""
        quota = self.get_quota(quota_type)
        if quota is None:
            return True  # No quota set means no limit
        return quota.use(amount)
    
    def check_violations(self) -> List[QuotaType]:
        """Check which quotas have been violated."""
        violated = []
        for quota_type, quota in self.quotas.items():
            if quota.violated:
                violated.append(quota_type)
        return violated
    
    def get_violation_message(self) -> str:
        """Get a descriptive message about quota violations."""
        violations = self.check_violations()
        if not violations:
            return "No quota violations"
        
        messages = []
        for quota_type in violations:
            quota = self.quotas[quota_type]
            messages.append(
                f"{quota_type.value}: {quota.current_usage:.2f}/{quota.limit:.2f} "
                f"({quota.get_usage_percent():.1f}%)"
            )
        
        return "Quota violations:\n" + "\n".join(messages)
    
    def is_within_all_quotas(self) -> bool:
        """Check if all quotas are within limits."""
        return len(self.check_violations()) == 0