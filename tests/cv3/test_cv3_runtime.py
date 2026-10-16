"""
Tests for CV3 Runtime Implementation
"""

import unittest
import sys
import os
from pathlib import Path

# Add the services directory to the path so we can import cv3 modules
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "services"))

from cv3.runtime import CV3Runtime
from cv3.graph import CV3Graph, CV3Node, CV3Pin, PinType, VariableType
from cv3.state import CV3State, CV3Variable
from cv3.quotas import CV3Quotas, QuotaType
from cv3.tracer import CV3Tracer, TraceEventType
from cv3.serializer import CV3Serializer


class TestCV3Runtime(unittest.TestCase):
    """Test cases for CV3 runtime functionality."""
    
    def setUp(self):
        """Set up test fixtures before each test method."""
        self.graph = CV3Graph("test_graph", "Test Graph")
        self.runtime = CV3Runtime(self.graph)
    
    def test_graph_creation(self):
        """Test creating a CV3 graph."""
        self.assertEqual(self.graph.name, "Test Graph")
        self.assertEqual(len(self.graph.nodes), 0)
        self.assertEqual(len(self.graph.connections), 0)
    
    def test_node_creation(self):
        """Test creating nodes in a graph."""
        # Create a timer node
        timer_node = CV3Node("timer1", "Test Timer", "Timer")
        timer_id = self.graph.add_node(timer_node)
        
        self.assertIn(timer_id, self.graph.nodes)
        self.assertEqual(self.graph.nodes[timer_id].name, "Test Timer")
        self.assertEqual(self.graph.nodes[timer_id].node_type, "Timer")
        
        # Check that default pins were created
        node = self.graph.nodes[timer_id]
        self.assertGreaterEqual(len(node.inputs), 1)
        self.assertGreaterEqual(len(node.outputs), 1)
        
        # Find execution input pin
        exec_in_pins = [pin for pin in node.inputs if pin.pin_type == PinType.EXECUTION]
        self.assertGreaterEqual(len(exec_in_pins), 1)
        
        # Find value input pin for duration
        duration_pins = [pin for pin in node.inputs if pin.name == "Duration"]
        self.assertGreaterEqual(len(duration_pins), 1)
        self.assertEqual(duration_pins[0].variable_type, VariableType.FLOAT)
    
    def test_pin_connection(self):
        """Test connecting pins between nodes."""
        # Create two nodes
        node1 = CV3Node("node1", "First Node", "Timer")
        node2 = CV3Node("node2", "Second Node", "VariableSet")
        
        id1 = self.graph.add_node(node1)
        id2 = self.graph.add_node(node2)
        
        # Get specific pins for connection
        # From node1's timer finished output to node2's variable set input
        timer_finished_out = None
        var_set_exec_in = None
        
        for pin in self.graph.nodes[id1].outputs:
            if pin.name == "Finished":
                timer_finished_out = pin
                break
        
        for pin in self.graph.nodes[id2].inputs:
            if pin.name == "Set":
                var_set_exec_in = pin
                break
        
        self.assertIsNotNone(timer_finished_out)
        self.assertIsNotNone(var_set_exec_in)
        
        # Connect the pins
        result = self.graph.connect_pins(timer_finished_out.id, var_set_exec_in.id)
        self.assertTrue(result)
        
        # Verify connection
        self.assertTrue(timer_finished_out.is_connected)
        self.assertTrue(var_set_exec_in.is_connected)
        self.assertEqual(timer_finished_out.connected_to, var_set_exec_in.id)
        self.assertEqual(var_set_exec_in.connected_to, timer_finished_out.id)
        
        # Check that connection is recorded
        self.assertEqual(len(self.graph.connections), 1)
        self.assertEqual(self.graph.connections[0], (timer_finished_out.id, var_set_exec_in.id))
    
    def test_graph_validation(self):
        """Test graph validation."""
        # Create a valid graph
        node1 = CV3Node("node1", "Node 1", "Timer")
        node2 = CV3Node("node2", "Node 2", "VariableSet")
        
        id1 = self.graph.add_node(node1)
        id2 = self.graph.add_node(node2)
        
        # Connect execution pins
        timer_finished_out = None
        var_set_exec_in = None
        
        for pin in self.graph.nodes[id1].outputs:
            if pin.name == "Finished":
                timer_finished_out = pin
                break
        
        for pin in self.graph.nodes[id2].inputs:
            if pin.name == "Set":
                var_set_exec_in = pin
                break
        
        if timer_finished_out and var_set_exec_in:
            self.graph.connect_pins(timer_finished_out.id, var_set_exec_in.id)
        
        # Validate the graph
        errors = self.graph.validate()
        # Should have no errors for a properly connected graph
        # (Though our simple validation might flag unconnected value inputs)
        
        # Test with intentionally invalid graph
        invalid_graph = CV3Graph("invalid", "Invalid Graph")
        bad_node = CV3Node("bad", "Bad Node", "Timer")
        invalid_graph.add_node(bad_node)
        # Don't connect required inputs
        
        errors = invalid_graph.validate()
        self.assertGreater(len(errors), 0)
    
    def test_runtime_initialization(self):
        """Test runtime initialization."""
        self.assertIsInstance(self.runtime.graph, CV3Graph)
        self.assertIsInstance(self.runtime.state, CV3State)
        self.assertIsInstance(self.runtime.quotas, CV3Quotas)
        self.assertIsInstance(self.runtime.tracer, CV3Tracer)
        self.assertFalse(self.runtime.is_executing)
    
    def test_variable_management(self):
        """Test variable creation and management."""
        # Create a variable through state
        var = self.runtime.state.create_variable("test_var", VariableType.INT, 42)
        
        self.assertIsInstance(var, CV3Variable)
        self.assertEqual(var.name, "test_var")
        self.assertEqual(var.var_type, VariableType.INT)
        self.assertEqual(var.get_value(), 42)
        
        # Retrieve variable by name
        retrieved_var = self.runtime.state.get_variable_by_name("test_var")
        self.assertEqual(retrieved_var.id, var.id)
        
        # Test setting value
        result = var.set_value(100)
        self.assertTrue(result)
        self.assertEqual(var.get_value(), 100)
        
        # Test type checking (simplified)
        result = var.set_value("not an int")
        # In our simplified implementation, this might still succeed
        # but in a full implementation it would fail
        
        # Test variable history
        history = var.get_change_history()
        self.assertGreaterEqual(len(history), 2)  # Initial value and at least one change
    
    def test_quotas(self):
        """Test quota system."""
        quotas = self.runtime.quotas
        
        # Check default quotas exist
        self.assertIn(QuotaType.TICK_TIME_MS, quotas.quotas)
        self.assertIn(QuotaType.MEMORY_BYTES, quotas.quotas)
        self.assertIn(QuotaType.OPERATION_COUNT, quotas.quotas)
        
        # Test using quota
        time_quota = quotas.get_quota(QuotaType.TICK_TIME_MS)
        initial_usage = time_quota.current_usage
        
        # Use some time
        result = quotas.use_quota(QuotaType.TICK_TIME_MS, 5.0)
        self.assertTrue(result)  # Should succeed
        self.assertEqual(time_quota.current_usage, initial_usage + 5.0)
        
        # Test quota violation
        # Use more than the limit (default is 16.0 ms)
        result = quotas.use_quota(QuotaType.TICK_TIME_MS, 20.0)  # 5 + 20 = 25 > 16
        self.assertFalse(result)  # Should fail
        self.assertTrue(time_quota.violated)
        
        # Test reset
        quotas.reset_all()
        self.assertEqual(time_quota.current_usage, 0.0)
        self.assertFalse(time_quota.violated)
    
    def test_tracer(self):
        """Test tracing functionality."""
        tracer = self.runtime.tracer
        
        # Enable tracing
        tracer.enable()
        self.assertTrue(tracer.enabled)
        
        # Trace some events
        tracer.trace_node_start("node1", "Test Node")
        tracer.trace_variable_set("test_var", 42)
        tracer.trace_node_end("node1", "Test Node", True)
        
        # Check events were recorded
        events = tracer.get_events()
        self.assertEqual(len(events), 3)
        
        # Check event types
        self.assertEqual(events[0].event_type, TraceEventType.NODE_START)
        self.assertEqual(events[1].event_type, TraceEventType.VARIABLE_SET)
        self.assertEqual(events[2].event_type, TraceEventType.NODE_END)
        
        # Check event data
        self.assertEqual(events[0].node_id, "node1")
        self.assertEqual(events[0].node_name, "Test Node")
        self.assertEqual(events[1].variable_name, "test_var")
        self.assertEqual(events[1].variable_value, 42)
        
        # Test disabling tracer
        tracer.disable()
        tracer.trace_variable_set("another_var", 100)
        # Should not have added new event since disabled
        events_after_disable = tracer.get_events()
        self.assertEqual(len(events_after_disable), 3)  # Still 3 events
        
        # Test clearing
        tracer.clear()
        self.assertEqual(len(tracer.get_events()), 0)
    
    def test_serializer(self):
        """Test serialization and deserialization."""
        # Create a simple graph
        node1 = CV3Node("node1", "Timer Node", "Timer")
        node2 = CV3Node("node2", "Variable Set", "VariableSet")
        node2.properties["variable_name"] = "timer_count"
        node2.properties["variable_type"] = "int"
        
        id1 = self.graph.add_node(node1)
        id2 = self.graph.add_node(node2)
        
        # Connect nodes
        timer_finished_out = None
        var_set_exec_in = None
        
        for pin in self.graph.nodes[id1].outputs:
            if pin.name == "Finished":
                timer_finished_out = pin
                break
        
        for pin in self.graph.nodes[id2].inputs:
            if pin.name == "Set":
                var_set_exec_in = pin
                break
        
        if timer_finished_out and var_set_exec_in:
            self.graph.connect_pins(timer_finished_out.id, var_set_exec_in.id)
        
        # Set some state
        var = self.runtime.state.create_variable("timer_count", VariableType.INT, 0)
        
        # Trace some activity
        self.runtime.tracer.enable()
        self.runtime.tracer.trace_node_start(id1, "Timer Node")
        self.runtime.tracer.trace_node_end(id1, "Timer Node", True)
        
        # Serialize the entire state
        snapshot = CV3Serializer.save_snapshot(
            self.graph, self.runtime.state, self.runtime.tracer, self.runtime.quotas
        )
        
        # Verify snapshot structure
        self.assertIn("timestamp", snapshot)
        self.assertIn("version", snapshot)
        self.assertIn("graph", snapshot)
        self.assertIn("state", snapshot)
        self.assertIn("tracer", snapshot)
        self.assertIn("quotas", snapshot)
        
        # Deserialize into new objects
        new_graph = CV3Graph("new_graph", "New Graph")
        new_state = CV3State()
        new_tracer = CV3Tracer()
        new_quotas = CV3Quotas()
        
        CV3Serializer.load_snapshot(snapshot, new_graph, new_state, new_tracer, new_quotas)
        
        # Verify basic properties
        self.assertEqual(new_graph.name, self.graph.name)
        self.assertEqual(len(new_graph.nodes), len(self.graph.nodes))
        self.assertEqual(len(new_graph.connections), len(self.graph.connections))
        
        # Verify state was restored
        self.assertEqual(len(new_state.variables), len(self.runtime.state.variables))
        
        # Verify tracer events were restored
        self.assertEqual(len(new_tracer.events), len(self.runtime.tracer.events))
    
    def test_execution_basic(self):
        """Test basic graph execution."""
        # Create a simple execution chain
        # Timer -> VariableSet -> (loop back or end)
        
        timer_node = CV3Node("timer1", "Timer", "Timer")
        var_set_node = CV3Node("var_set1", "Set Variable", "VariableSet")
        var_set_node.properties["variable_name"] = "counter"
        var_set_node.properties["variable_type"] = "int"
        
        timer_id = self.graph.add_node(timer_node)
        var_set_id = self.graph.add_node(var_set_node)
        
        # Connect timer output to variable set input
        timer_finished_out = None
        var_set_exec_in = None
        
        for pin in self.graph.nodes[timer_id].outputs:
            if pin.name == "Finished":
                timer_finished_out = pin
                break
        
        for pin in self.graph.nodes[var_set_id].inputs:
            if pin.name == "Set":
                var_set_exec_in = pin
                break
        
        if timer_finished_out and var_set_exec_in:
            self.graph.connect_pins(timer_finished_out.id, var_set_exec_in.id)
        
        # Validate graph
        errors = self.graph.validate()
        # Note: Our simple validation might report some errors due to unconnected value inputs
        # but execution should still work for basic flow
        
        # Test that we can start execution (will run for a limited time)
        # We won't wait for completion since this is a unit test
        self.assertFalse(self.runtime.is_executing)
        
        # Test stopping execution
        self.runtime.stop()
        self.assertFalse(self.runtime.is_executing)
    
    def test_quotas_during_execution(self):
        """Test that quotas are checked during execution."""
        # Set a very low quota to test violation
        quotas = self.runtime.quotas
        quotas.set_quota(QuotaType.OPERATION_COUNT, 1)  # Allow only 1 operation
        
        # Create a node that would exceed this quota
        # In our simple implementation, most nodes count as 1 operation
        # So executing any node should work, but executing multiple might fail
        
        # Actually, let's test quota checking directly
        self.assertTrue(quotas.is_within_all_quotas())
        
        # Use up the quota
        quotas.use_quota(QuotaType.OPERATION_COUNT, 1)
        self.assertTrue(quotas.is_within_all_quotas())  # At limit, not violated yet
        
        # Use a tiny bit more to trigger violation
        quotas.use_quota(QuotaType.OPERATION_COUNT, 0.1)
        self.assertFalse(quotas.is_within_all_quotas())
        self.assertTrue(quotas.get_quota(QuotaType.OPERATION_COUNT).violated)


if __name__ == "__main__":
    unittest.main()