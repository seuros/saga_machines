# frozen_string_literal: true
require 'minitest/autorun'
require 'saga_machines'

class SagaTest < Minitest::Test
  def test_forward_completion_and_terminal_boundaries
    saga = SagaMachines::Saga.new(7, steps: 8)
    assert_equal 8, saga.steps
    8.times do |step|
      action = saga.next_action
      assert_equal [step, :forward], [action.step, action.kind]
      assert_equal [:forward, step], saga.progress
      assert action.same_key?(saga.next_action)
      refute saga.cancel
      assert saga.resolve(action, :success)
      refute saga.resolve(action, :success)
    end
    assert_equal [:complete, nil], saga.progress
    assert_nil saga.next_action
    refute saga.cancel
  end

  def test_reverse_compensation_and_retry
    saga = SagaMachines::Saga.new(1, steps: 4)
    3.times { assert saga.resolve(saga.next_action, :success) }
    failed = saga.next_action
    assert saga.resolve(failed, :failure)
    assert_equal [:compensating, 2], saga.progress
    old = saga.next_action
    assert_equal :compensate, old.kind
    assert saga.resolve(old, :failure)
    retry_action = saga.next_action
    assert old.same_key?(retry_action)
    refute saga.resolve(old, :success)
    assert saga.resolve(retry_action, :success)
    [1, 0].each do |step|
      action = saga.next_action
      assert_equal step, action.step
      assert saga.resolve(action, :success)
    end
    assert_equal [:aborted, nil], saga.progress
    assert_nil saga.next_action
    refute saga.cancel
  end

  def test_cancel_between_actions_only
    fresh = SagaMachines::Saga.new(1, steps: 1)
    assert fresh.cancel
    assert_equal [:aborted, nil], fresh.progress
    saga = SagaMachines::Saga.new(2, steps: 2)
    assert saga.resolve(saga.next_action, :success)
    assert saga.cancel
    assert_equal [:compensating, 0], saga.progress
    refute saga.cancel
    assert saga.resolve(saga.next_action, :success)
    assert_equal [:aborted, nil], saga.progress
  end

  def test_opaque_cross_owner_same_identity_and_gc
    a, b = SagaMachines::Saga.new(9, steps: 2), SagaMachines::Saga.new(9, steps: 2)
    action = a.next_action
    other = b.next_action
    assert action.same_key?(other)
    refute b.resolve(action, :success)
    assert b.resolve(other, :success)
    assert_raises(TypeError) { SagaMachines::Action.allocate }
    assert_raises(TypeError) { action.dup }
    a = nil
    GC.start
    replacement = SagaMachines::Saga.new(9, steps: 2)
    replacement.next_action
    refute replacement.resolve(action, :success)
  end

  def test_validation_and_freeze
    [1.0, nil, '1', Float::NAN, Float::INFINITY].each do |v|
      assert_raises(TypeError) { SagaMachines::Saga.new(v, steps: 1) }
      assert_raises(TypeError) { SagaMachines::Saga.new(1, steps: v) }
    end
    [-1, 2**64].each { |v| assert_raises(RangeError) { SagaMachines::Saga.new(v, steps: 1) } }
    [5, 7, 17].each { |v| assert_raises(ArgumentError) { SagaMachines::Saga.new(1, steps: v) } }
    saga = SagaMachines::Saga.new(2**64 - 1, steps: 1)
    action = saga.next_action
    assert_raises(ArgumentError) { saga.resolve(action, :timeout) }
    assert_raises(TypeError) { saga.resolve(action, 'success') }
    assert_equal [:forward, 0], saga.progress
    saga.freeze
    assert_raises(FrozenError) { saga.next_action }
    assert_raises(FrozenError) { saga.cancel }
    assert_raises(FrozenError) { saga.resolve(action, :success) }
    assert_equal 0, action.step
    assert_equal :forward, action.kind
  end

  def test_supported_exact_lengths_without_padding
    [0, 1, 2, 3, 4, 8, 16].each do |steps|
      saga = SagaMachines::Saga.new(1, steps: steps)
      assert_equal steps, saga.steps
      steps.times do |step|
        action = saga.next_action
        assert_equal step, action.step
        assert saga.resolve(action, :success)
      end
      assert_nil saga.next_action
      assert_equal [:complete, nil], saga.progress
    end
  end

  def test_gc_stress_and_repeated_attempt
    previous = GC.stress
    GC.stress = true
    saga = SagaMachines::Saga.new(1, steps: 1)
    action = saga.next_action
    repeated = saga.next_action
    GC.compact
    assert action.same_key?(repeated)
    assert saga.resolve(repeated, :success)
    refute saga.resolve(action, :failure)
    assert_equal [:complete, nil], saga.progress
  ensure
    GC.stress = previous
  end
end
