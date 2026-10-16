"""
CV3 Runtime
Main execution engine for Circuits V3 graphs.
"""

from typing import Dict, List, Optional, Any, Callable
import time
import uuid
from .graph import CV3Graph, CV3Node, CV3Pin, PinType
from .state import CV3State, CV3Variable
from .quotas import CV3Quotas, QuotaType
from .tracer import CV3Tracer, TraceEventType
from .serializer import CV3Serializer


class CV3Runtime:
    """Main runtime for executing CV3 graphs deterministically."""
    
    def __init__(self, graph: CV3Graph):
        self.graph = graph
        self.state = CV3State()
        self.quotas = CV3Quotas()
        self.tracer = CV3Tracer()
        self.is_executing = False
        self.execution_callbacks: List[Callable[[str, Any], None]] = []
        self._node_executors: Dict[str, Callable] = {}
        self._register_default_executors()
    
    def _register_default_executors(self):
        """Register default node executors for common node types."""
        self._node_executors["Timer"] = self._execute_timer
        self._node_executors["VariableSet"] = self._execute_variable_set
        self._node_executors["VariableGet"] = self._execute_variable_get
        self._node_executors["MathAdd"] = self._execute_math_add
        self._node_executors["Delay"] = self._execute_delay
        self._node_executors["Random"] = self._execute_random
        self._node_executors["Print"] = self._execute_print
    
    def register_executor(self, node_type: str, executor: Callable[[CV3Node, CV3State, CV3Tracer, CV3Quotas], bool]):
        """Register a custom executor for a node type."""
        self._node_executors[node_type] = executor
    
    def add_execution_callback(self, callback: Callable[[str, Any], None]):
        """Add a callback to be called when nodes produce output."""
        self.execution_callbacks.append(callback)
    
    def remove_execution_callback(self, callback: Callable[[str, Any], None]):
        """Remove an execution callback."""
        if callback in self.execution_callbacks:
            self.execution_callbacks.remove(callback)
    
    def validate_graph(self) -> List[str]:
        """Validate the graph before execution."""
        errors = self.graph.validate()
        self.tracer.trace_graph_validation(len(errors) == 0, errors)
        return errors
    
    def execute(self, max_ticks: int = 1000) -> bool:
        """Execute the graph deterministically."""
        if self.is_executing:
            raise RuntimeError("Graph is already executing")
        
        # Validate graph first
        validation_errors = self.validate_graph()
        if validation_errors:
            return False
        
        self.is_executing = True
        self.state.reset_tick_count()
        self.state.set_start_time(time.time())
        self.quotas.reset_all()
        self.tracer.enable()
        self.tracer.trace_execution_start()
        
        try:
            # Find entry points (nodes with no execution inputs or triggered externally)
            # For simplicity, we'll execute nodes that have execution inputs connected
            # In a full implementation, we'd have a proper execution scheduler
            
            tick = 0
            while tick < max_ticks and self.is_executing:
                tick_start = time.time()
                
                # Reset quotas for this tick
                self.quotas.reset_all()
                
                # Execute nodes that are ready
                executed_any = self._execute_tick()
                
                # Check quotas
                if not self.quotas.is_within_all_quotas():
                    self.tracer.trace_execution_end(False)
                    return False
                
                # Trace tick completion
                tick_time = (time.time() - tick_start) * 1000  # Convert to ms
                self.quotas.get_quota(QuotaType.TICK_TIME_MS).use(tick_time)
                self.state.increment_tick()
                
                # If nothing was executed, we might be done or waiting
                if not executed_any:
                    # In a full implementation, we'd check for external triggers
                    # For now, break after a few idle ticks
                    if tick > 10:  # Allow some idle time
                        break
                
                tick += 1
                
                # Small delay to prevent CPU spinning in tight loops
                if tick_time < 1.0:  # If tick took less than 1ms
                    time.sleep(0.001)  # Sleep for 1ms
            
            self.tracer.trace_execution_end(True)
            return True
            
        except Exception as e:
            self.tracer.trace_execution_end(False)
            print(f"Execution error: {e}")
            return False
        finally:
            self.is_executing = False
    
    def stop(self):
        """Stop graph execution."""
        self.is_executing = False
    
    def _execute_tick(self) -> bool:
        """Execute one tick of the graph."""
        executed_any = False
        
        # For each node, check if it's ready to execute
        for node_id, node in self.graph.nodes.items():
            if self._is_node_ready(node):
                if self._execute_node(node):
                    executed_any = True
        
        return executed_any
    
    def _is_node_ready(self, node: CV3Node) -> bool:
        """Check if a node is ready to execute."""
        # Check if all required inputs are connected and have values
        for pin in node.inputs:
            if pin.pin_type == PinType.EXECUTION:
                # Execution pins need to be triggered
                # In a simple implementation, we'll consider them ready if connected
                if not pin.is_connected:
                    return False
            elif pin.pin_type == PinType.VALUE:
                # Value pins need to have a value
                # In a simple implementation, we'll assume connected pins have values
                if not pin.is_connected:
                    return False
        return True
    
    def _execute_node(self, node: CV3Node) -> bool:
        """Execute a single node."""
        self.state.push_execution(node.id)
        self.tracer.trace_node_start(node.id, node.name)
        
        try:
            # Get executor for this node type
            executor = self._node_executors.get(node.node_type)
            if executor is None:
                # Default executor for basic nodes
                success = self._execute_default(node)
            else:
                success = executor(node, self.state, self.tracer, self.quotas)
            
            self.tracer.trace_node_end(node.id, node.name, success)
            
            # Propagate execution to connected output pins
            if success:
                self._propagate_execution(node)
            
            return success
            
        except Exception as e:
            self.tracer.trace_node_end(node.id, node.name, False)
            print(f"Error executing node {node.name}: {e}")
            return False
        finally:
            self.state.pop_execution()
    
    def _execute_default(self, node: CV3Node) -> bool:
        """Default executor for nodes without specific executors."""
        # For most nodes, just propagate execution
        self._propagate_execution(node)
        return True
    
    def _propagate_execution(self, node: CV3Node):
        """Propagate execution signal to connected output pins."""
        for pin in node.outputs:
            if pin.pin_type == PinType.EXECUTION and pin.is_connected:
                # Find the connected node and input pin
                connected_pin_id = pin.connected_to
                for node_id, target_node in self.graph.nodes.items():
                    for target_pin in target_node.inputs:
                        if target_pin.id == connected_pin_id:
                            # In a full implementation, we'd mark this as ready to execute
                            # For now, we'll just note that execution happened
                            pass
    
    def _notify_outputs(self, node: CV3Node, output_data: Dict[str, Any]):
        """Notify callbacks about node outputs."""
        for callback in self.execution_callbacks:
            try:
                callback(node.id, output_data)
            except Exception:
                pass  # Don't let callback errors break execution
    
    # Specific node executors
    def _execute_timer(self, node: CV3Node, state: CV3State, tracer: CV3Tracer, quotas: CV3Quotas) -> bool:
        """Execute a timer node."""
        # Get duration from input
        duration_pin = None
        for pin in node.inputs:
            if pin.name == "Duration":
                duration_pin = pin
                break
        
        if duration_pin and duration_pin.is_connected:
            # In a full implementation, we'd get the actual value
            # For now, we'll use a default
            duration = 1.0  # seconds
        else:
            duration = 1.0
        
        # Simple timer: if we've been executing for duration seconds, trigger output
        elapsed = state.get_elapsed_time()
        if elapsed >= duration:
            # Trigger finished output
            return True
        
        # Continue ticking
        return True
    
    def _execute_variable_set(self, node: CV3Node, state: CV3State, tracer: CV3Tracer, quotas: CV3Quotas) -> bool:
        """Execute a variable set node."""
        # Get value from input
        value_pin = None
        exec_pin = None
        for pin in node.inputs:
            if pin.name == "Value":
                value_pin = pin
            elif pin.name == "Set":
                exec_pin = pin
        
        # In a full implementation, we'd get the actual value from connected nodes
        # For now, we'll set a test value
        test_value = 42
        
        # Find or create variable
        var_name = node.properties.get("variable_name", "test_var")
        variable = state.get_variable_by_name(var_name)
        if variable is None:
            # Determine type from properties or default to int
            var_type_str = node.properties.get("variable_type", "int")
            from .graph import VariableType
            try:
                var_type = VariableType(var_type_str)
            except ValueError:
                var_type = VariableType.INT
            variable = state.create_variable(var_name, var_type, test_value)
        else:
            variable.set_value(test_value)
        
        tracer.trace_variable_set(var_name, test_value)
        
        # Check variable quota
        quotas.get_quota(QuotaType.VARIABLE_COUNT).use(len(state.variables))
        
        return True
    
    def _execute_variable_get(self, node: CV3Node, state: CV3State, tracer: CV3Tracer, quotas: CV3Quotas) -> bool:
        """Execute a variable get node."""
        var_name = node.properties.get("variable_name", "test_var")
        variable = state.get_variable_by_name(var_name)
        
        if variable is not None:
            value = variable.get_value()
            tracer.trace_variable_get(var_name, value)
            return True
        else:
            # Variable doesn't exist yet
            tracer.trace_variable_get(var_name, None)
            return False
    
    def _execute_math_add(self, node: CV3Node, state: CV3State, tracer: CV3Tracer, quotas: CV3Quotas) -> bool:
        """Execute a math addition node."""
        # Get inputs
        a_pin = None
        b_pin = None
        exec_pin = None
        for pin in node.inputs:
            if pin.name == "A":
                a_pin = pin
            elif pin.name == "B":
                b_pin = pin
            elif pin.name == "Add":
                exec_pin = pin
        
        # Get values (simplified)
        a_val = 10.0
        b_val = 5.0
        result = a_val + b_val
        
        # Store result in a variable if specified
        result_var_name = node.properties.get("result_variable")
        if result_var_name:
            variable = state.get_variable_by_name(result_var_name)
            if variable is None:
                variable = state.create_variable(result_var_name, VariableType.FLOAT, result)
            else:
                variable.set_value(result)
            tracer.trace_variable_set(result_var_name, result)
        
        return True
    
    def _execute_delay(self, node: CV3Node, state: CV3State, tracer: CV3Tracer, quotas: CV3Quotas) -> bool:
        """Execute a delay node."""
        # Simple delay - just return true to continue ticking
        # In a full implementation, this would wait for a specified time
        return True
    
    def _execute_random(self, node: CV3Node, state: CV3State, tracer: CV3Tracer, quotas: CV3Quotas) -> bool:
        """Execute a random number node."""
        import random
        min_val = node.properties.get("min", 0)
        max_val = node.properties.get("max", 100)
        result = random.uniform(min_val, max_val)
        
        # Store result
        result_var_name = node.properties.get("result_variable", "random_result")
        variable = state.get_variable_by_name(result_var_name)
        if variable is None:
            variable = state.create_variable(result_var_name, VariableType.FLOAT, result)
        else:
            variable.set_value(result)
        
        tracer.trace_variable_set(result_var_name, result)
        return True
    
    def _execute_print(self, node: CV3Node, state: CV3State, tracer: CV3Tracer, quotas: CV3Quotas) -> bool:
        """Execute a print node."""
        message = node.properties.get("message", "Hello from CV3!")
        print(f"[CV3 PRINT] {message}")
        tracer.trace_variable_set("last_print", message)
        return True
    
    def save_state(self) -> Dict[str, Any]:
        """Save the current execution state."""
        return CV3Serializer.save_snapshot(
            self.graph, self.state, self.tracer, self.quotas
        )
    
    def load_state(self, snapshot: Dict[str, Any]):
        """Load a previously saved execution state."""
        CV3Serializer.load_snapshot(snapshot, self.graph, self.state, self.tracer, self.quotas)
    
    def get_timeline(self) -> List[Dict[str, Any]]:
        """Get execution timeline for profiling/debugging."""
        return self.tracer.get_execution_timeline()
    
    def get_stats(self) -> Dict[str, Any]:
        """Get execution statistics."""
        return {
            "tick_count": self.state.tick_count,
            "elapsed_time": self.state.get_elapsed_time(),
            "variables_count": len(self.state.variables),
            "quotas": {
                quota_type.value: {
                    "limit": quota.limit,
                    "usage": quota.current_usage,
                    "percent": quota.get_usage_percent(),
                    "violated": quota.violated
                }
                for quota_type, quota in self.quotas.quotas.items()
            },
            "trace_events": len(self.tracer.events)
        }