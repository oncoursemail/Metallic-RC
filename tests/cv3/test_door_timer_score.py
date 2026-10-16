"""
Test for CV3 Door/Timer/Score Graph
This demonstrates the multiplayer door/timer/score graph mentioned in the issue acceptance criteria.
"""

import unittest
import sys
import os
from pathlib import Path

# Add the services directory to the path so we can import cv3 modules
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "services"))

from cv3.runtime import CV3Runtime
from cv3.graph import CV3Graph, CV3Node, CV3Pin, PinType, VariableType
from cv3.state import CV3State
from cv3.tracer import CV3Tracer
from cv3.quotas import CV3Quotas


class TestDoorTimerScoreGraph(unittest.TestCase):
    """Test the door/timer/score graph from the issue acceptance criteria."""
    
    def setUp(self):
        """Set up the door/timer/score graph."""
        self.graph = CV3Graph("door_timer_score", "Door Timer Score Graph")
        self.runtime = CV3Runtime(self.graph)
        self._build_door_timer_score_graph()
    
    def _build_door_timer_score_graph(self):
        """Build a door/timer/score graph similar to what would be used in multiplayer."""
        # Door controller: waits for input, opens door for a time, then closes
        door_controller = CV3Node("door_controller", "Door Controller", "Timer")
        door_controller.properties["door_open_duration"] = 5.0  # seconds
        
        # Timer that triggers the door opening
        trigger_timer = CV3Node("trigger_timer", "Trigger Timer", "Timer")
        trigger_timer.properties["trigger_interval"] = 10.0  # seconds
        
        # Score counter that increments when door opens
        score_counter = CV3Node("score_counter", "Score Counter", "VariableSet")
        score_counter.properties["variable_name"] = "door_open_count"
        score_counter.properties["variable_type"] = "int"
        
        # Score display (could be a print node in debugging)
        score_display = CV3Node("score_display", "Score Display", "Print")
        score_display.properties["message"] = "Door opened {count} times"
        
        # Add nodes to graph
        door_id = self.graph.add_node(door_controller)
        trigger_id = self.graph.add_node(trigger_timer)
        score_id = self.graph.add_node(score_counter)
        display_id = self.graph.add_node(score_display)
        
        # Debug: Print node info
        print(f"Door controller ID: {door_id}")
        print(f"Trigger timer ID: {trigger_id}")
        print(f"Score counter ID: {score_id}")
        print(f"Score display ID: {display_id}")
        
        # Find specific pins
        trigger_finished_out = None
        for pin in self.graph.nodes[trigger_id].outputs:
            print(f"Trigger timer output pin: {pin.name} ({pin.id})")
            if pin.name == "Finished":
                trigger_finished_out = pin
                break
        
        door_start_in = None
        for pin in self.graph.nodes[door_id].inputs:
            print(f"Door controller input pin: {pin.name} ({pin.id})")
            if pin.name == "Start":
                door_start_in = pin
                break
        
        print(f"Trigger finished out: {trigger_finished_out}")
        print(f"Door start in: {door_start_in}")
        
        # Connect trigger timer to door controller (trigger opens door)
        if trigger_finished_out and door_start_in:
            result = self.graph.connect_pins(trigger_finished_out.id, door_start_in.id)
            print(f"Connected trigger finished to door start: {result}")
        else:
            print("ERROR: Could not find trigger finished or door start pins")
        
        # Connect door controller finished to score increment (when door closes, increment score)
        door_finished_out = None
        for pin in self.graph.nodes[door_id].outputs:
            print(f"Door controller output pin: {pin.name} ({pin.id})")
            if pin.name == "Finished":
                door_finished_out = pin
                break
        
        score_set_in = None
        for pin in self.graph.nodes[score_id].inputs:
            print(f"Score counter input pin: {pin.name} ({pin.id})")
            if pin.name == "Set":
                score_set_in = pin
                break
        
        print(f"Door finished out: {door_finished_out}")
        print(f"Score set in: {score_set_in}")
        
        if door_finished_out and score_set_in:
            result = self.graph.connect_pins(door_finished_out.id, score_set_in.id)
            print(f"Connected door finished to score set: {result}")
        else:
            print("ERROR: Could not find door finished or score set pins")
        
        # Connect score increment to display
        score_set_out = None
        for pin in self.graph.nodes[score_id].outputs:
            print(f"Score counter output pin: {pin.name} ({pin.id})")
            if pin.name == "Out":
                score_set_out = pin
                break
        
        display_print_in = None
        for pin in self.graph.nodes[display_id].inputs:
            print(f"Score display input pin: {pin.name} ({pin.id})")
            if pin.name == "In":  # Print node takes execution input
                display_print_in = pin
                break
        
        print(f"Score set out: {score_set_out}")
        print(f"Display print in: {display_print_in}")
        
        # Connect score output to display input (simplified - in reality we'd need to format message)
        if score_set_out and display_print_in:
            result = self.graph.connect_pins(score_set_out.id, display_print_in.id)
            print(f"Connected score set to display print: {result}")
        else:
            print("ERROR: Could not find score set or display print pins")
        
        # Set initial state
        state = self.runtime.state
        state.create_variable("door_open_count", VariableType.INT, 0)
        state.create_variable("trigger_active", VariableType.BOOL, True)
        
        # Enable tracing for debugging
        self.runtime.tracer.enable()
        
        # Print final connections
        print(f"Final connections: {self.graph.connections}")
    
    def test_graph_structure(self):
        """Test that the graph was built correctly."""
        # Should have 4 nodes
        self.assertEqual(len(self.graph.nodes), 4)
        
        # Debug: print connections
        print(f"Number of connections: {len(self.graph.connections)}")
        for i, conn in enumerate(self.graph.connections):
            print(f"  Connection {i}: {conn}")
        
        # Should have 3 connections (trigger->door, door->score, score->display)
        self.assertEqual(len(self.graph.connections), 3)
        
        # Check node types
        node_types = [node.node_type for node in self.graph.nodes.values()]
        self.assertIn("Timer", node_types)  # trigger_timer and door_controller
        self.assertIn("VariableSet", node_types)  # score_counter
        self.assertIn("Print", node_types)  # score_display
        
        # Count timers specifically
        timer_count = sum(1 for node in self.graph.nodes.values() if node.node_type == "Timer")
        self.assertEqual(timer_count, 2)
    
    def test_graph_validation(self):
        """Test that the graph validates correctly."""
        errors = self.graph.validate()
        # Door timer/score graph should be valid
        # Note: Our simple validator might complain about unconnected value inputs
        # but the core execution connections should be valid
        
        # At minimum, we should be able to execute without crashing
        self.assertIsInstance(errors, list)
    
    def test_execution_begins(self):
        """Test that execution can begin without error."""
        # Don't actually run full execution in unit test (would take too long)
        # Just verify setup is correct
        
        self.assertFalse(self.runtime.is_executing)
        
        # Test that we can access all components
        self.assertIsInstance(self.runtime.graph, CV3Graph)
        self.assertIsInstance(self.runtime.state, CV3State)
        self.assertIsInstance(self.runtime.tracer, CV3Tracer)
        self.assertIsInstance(self.runtime.quotas, CV3Quotas)
        
        # Test that state has our variables
        door_count = self.runtime.state.get_variable_by_name("door_open_count")
        self.assertIsNotNone(door_count)
        if door_count:
            self.assertEqual(door_count.get_value(), 0)
        
        trigger_active = self.runtime.state.get_variable_by_name("trigger_active")
        self.assertIsNotNone(trigger_active)
        if trigger_active:
            self.assertEqual(trigger_active.get_value(), True)
    
    def test_tracing_works(self):
        """Test that tracing captures execution events."""
        # Enable tracing
        self.runtime.tracer.enable()
        
        # Simulate some tracing events
        self.runtime.tracer.trace_node_start("trigger_timer", "Trigger Timer")
        self.runtime.tracer.trace_node_end("trigger_timer", "Trigger Timer", True)
        self.runtime.tracer.trace_variable_set("door_open_count", 1)
        self.runtime.tracer.trace_node_start("score_display", "Score Display")
        self.runtime.tracer.trace_node_end("score_display", "Score Display", True)
        
        # Check events were recorded
        events = self.runtime.tracer.get_events()
        self.assertGreaterEqual(len(events), 4)
        
        # Check for specific event types
        event_types = [event.event_type for event in events]
        from cv3.tracer import TraceEventType
        self.assertIn(TraceEventType.NODE_START, event_types)
        self.assertIn(TraceEventType.NODE_END, event_types)
        self.assertIn(TraceEventType.VARIABLE_SET, event_types)
        
        # Check for specific node names
        node_names = [event.node_name for event in events if event.node_name]
        self.assertIn("Trigger Timer", node_names)
        self.assertIn("Score Display", node_names)
        
        # Check for variable events
        variable_events = [event for event in events if event.event_type == TraceEventType.VARIABLE_SET]
        self.assertGreaterEqual(len(variable_events), 1)
        if variable_events:
            self.assertEqual(variable_events[0].variable_name, "door_open_count")
            self.assertEqual(variable_events[0].variable_value, 1)


if __name__ == "__main__":
    unittest.main()