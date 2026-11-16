# frozen_string_literal: true
require 'minitest/autorun'
require 'saga_machines'

module SagaHelpers
  include SagaMachines

  ID = SagaMachines::SagaId.new(7)

  # Everything a saga holds: a refused call must leave it unchanged.
  def state(saga)
    [saga.steps, saga.snapshot, saga.pending]
  end

  def assert_unchanged(saga)
    before = state(saga)
    yield
    assert_equal before, state(saga)
  end

  # Issue the next action and resolve it with `outcome`.
  def run_step(saga, outcome)
    action = saga.next_action
    assert_instance_of Action, action
    assert_equal :resolved, saga.resolve(action, outcome)
    action
  end

  def action(step, kind, attempt, saga: ID)
    Action.new(ActionKey.new(saga, step, kind), AttemptId.new(attempt))
  end

  # Runs the block with each method of `klass` named in `bodies` replaced by
  # its body, then restores the originals.
  def redefining(klass, **bodies)
    originals = bodies.to_h { |name, _| [name, klass.instance_method(name)] }
    bodies.each { |name, body| replace_method(klass, name, body) }
    yield
  ensure
    originals&.each { |name, method| replace_method(klass, name, (method if method.owner == klass)) }
  end

  # Defines `name` on `klass` as `body` (removes it when `body` is nil),
  # removing the method `klass` itself holds first. Ruby warns about the
  # redefinition, deliberate here, so warnings are off for the swap alone.
  def replace_method(klass, name, body)
    verbose = $VERBOSE
    $VERBOSE = nil
    klass.remove_method(name) if klass.method_defined?(name, false) || klass.private_method_defined?(name, false)
    klass.define_method(name, body) if body
  ensure
    $VERBOSE = verbose
  end
end
